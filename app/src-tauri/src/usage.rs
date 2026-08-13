//! Reads Claude Code's cached OAuth token and polls the (undocumented)
//! usage endpoint for session/weekly limit percentages.
//!
//! Findings this is built on: docs/phase-0-findings.md

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

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

pub async fn fetch_usage(client: &reqwest::Client, token: &str) -> Result<UsageSnapshot> {
    let response = client
        .get(USAGE_ENDPOINT)
        .bearer_auth(token)
        .header("anthropic-beta", OAUTH_BETA_HEADER)
        .send()
        .await
        .context("request to usage endpoint failed")?;

    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("usage endpoint returned HTTP {status}"));
    }

    let raw: RawUsageResponse = response
        .json()
        .await
        .context("usage endpoint response did not match expected shape")?;

    Ok(UsageSnapshot {
        limits: raw.limits,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// One poll attempt: read the token fresh each time (cheap, and tolerates
/// token refresh/rotation between polls) and fetch usage.
pub async fn poll_once(client: &reqwest::Client) -> Result<UsageSnapshot> {
    let token = read_access_token()?;
    fetch_usage(client, &token).await
}
