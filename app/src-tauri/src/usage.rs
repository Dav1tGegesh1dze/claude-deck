//! Reads Claude Code's cached OAuth token and polls the (undocumented)
//! usage endpoint for session/weekly limit percentages.
//!
//! Findings this is built on: docs/phase-0-findings.md

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Distinguishes "the endpoint rate-limited us" from other failures, so
/// callers can back off instead of just logging-and-retrying-at-normal-pace.
/// See ROADMAP.md "Known issues" - a low refresh interval reliably
/// triggers this endpoint's own undocumented rate limit.
#[derive(Debug)]
pub enum PollError {
    RateLimited { retry_after_secs: Option<u64> },
    Other(anyhow::Error),
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::RateLimited {
                retry_after_secs: Some(s),
            } => write!(f, "rate limited by usage endpoint (HTTP 429), retry after {s}s"),
            PollError::RateLimited {
                retry_after_secs: None,
            } => write!(f, "rate limited by usage endpoint (HTTP 429)"),
            PollError::Other(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for PollError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitEntry {
    pub kind: String,
    pub percent: f64,
    pub severity: String,
    pub resets_at: Option<String>,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub limits: Vec<LimitEntry>,
    pub fetched_at: String,
}

#[derive(Deserialize)]
struct RawUsageResponse {
    limits: Vec<LimitEntry>,
}

#[derive(Deserialize)]
struct CredentialFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OauthBlob>,
}

#[derive(Deserialize)]
struct OauthBlob {
    #[serde(rename = "accessToken")]
    access_token: String,
}

/// Reads the OAuth access token Claude Code already cached after `claude
/// login`. Platform-dependent: macOS uses Keychain, everything else is
/// presumed to use `~/.claude/.credentials.json` (unverified — see
/// docs/phase-0-findings.md).
pub fn read_access_token() -> Result<String> {
    let raw = read_credential_blob()?;
    let parsed: CredentialFile = serde_json::from_str(&raw)
        .context("credential blob was not the expected JSON shape")?;
    let oauth = parsed
        .claude_ai_oauth
        .ok_or_else(|| anyhow!("no claudeAiOauth field in credential blob"))?;
    Ok(oauth.access_token)
}

#[cfg(target_os = "macos")]
fn read_credential_blob() -> Result<String> {
    let user = std::env::var("USER").context("USER env var not set")?;
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &user)
        .context("failed to open Keychain entry")?;
    entry.get_password().context(
        "no Claude Code credentials found in Keychain — run `claude login` first",
    )
}

#[cfg(not(target_os = "macos"))]
fn read_credential_blob() -> Result<String> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    let path = home.join(".claude").join(".credentials.json");
    std::fs::read_to_string(&path).with_context(|| {
        format!(
            "no Claude Code credentials found at {} — run `claude login` first",
            path.display()
        )
    })
}

pub async fn fetch_usage(client: &reqwest::Client, token: &str) -> Result<UsageSnapshot, PollError> {
    let response = client
        .get(USAGE_ENDPOINT)
        .bearer_auth(token)
        .header("anthropic-beta", OAUTH_BETA_HEADER)
        .send()
        .await
        .map_err(|e| PollError::Other(anyhow!("request to usage endpoint failed: {e}")))?;

    let status = response.status();

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after_secs = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        return Err(PollError::RateLimited { retry_after_secs });
    }

    if !status.is_success() {
        return Err(PollError::Other(anyhow!(
            "usage endpoint returned HTTP {status}"
        )));
    }

    let raw: RawUsageResponse = response.json().await.map_err(|e| {
        PollError::Other(anyhow!(
            "usage endpoint response did not match expected shape: {e}"
        ))
    })?;

    Ok(UsageSnapshot {
        limits: raw.limits,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Polls usage, reusing a cached token when possible so the OS credential
/// store (macOS Keychain, which prompts the user) isn't touched on every
/// single poll tick — only once, plus a re-read if the cached token stops
/// working (expired/rotated). This was a real problem found during
/// hardware testing: re-reading Keychain every poll trained the "Always
/// Allow" grant to never stick, since it kept getting asked again.
pub async fn poll_cached(
    client: &reqwest::Client,
    cached_token: &tokio::sync::Mutex<Option<String>>,
) -> Result<UsageSnapshot, PollError> {
    let existing = cached_token.lock().await.clone();

    if let Some(token) = existing {
        match fetch_usage(client, &token).await {
            Ok(snapshot) => return Ok(snapshot),
            // Don't mask rate limiting by silently retrying with a fresh
            // token read - that just wastes a Keychain touch and gets
            // limited again.
            Err(err @ PollError::RateLimited { .. }) => return Err(err),
            Err(PollError::Other(_)) => {
                // Cached token might be stale/expired/rotated - fall
                // through to a fresh read from the credential store.
            }
        }
    }

    let fresh = read_access_token().map_err(PollError::Other)?;
    let snapshot = fetch_usage(client, &fresh).await?;
    *cached_token.lock().await = Some(fresh);
    Ok(snapshot)
}
