//! Local app config, stored as JSON under the OS app-data dir. No cloud
//! sync, no telemetry (SPEC.md §7).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Session,
    Weekly,
    Budget,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonAssignment {
    pub metric: Metric,
}

impl Default for ButtonAssignment {
    fn default() -> Self {
        Self {
            metric: Metric::None,
        }
    }
}

/// A user-defined soft cap layered on top of the real weekly limit — not
/// something Anthropic enforces. See SPEC.md §6.3 and budget.rs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub enabled: bool,
    /// How many percentage-points of the weekly budget the user wants to
    /// avoid burning in a single day.
    pub daily_cap_percent: f64,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            daily_cap_percent: 20.0,
        }
    }
}

/// Floor for `refresh_interval_secs`. The usage endpoint's own
/// undocumented rate limiting kicks in well before this on real-world
/// evidence (a user hit HTTP 429 at 50s) - a known-working reference
/// implementation defaults to 300s and explicitly warns against low
/// values. See ROADMAP.md "Known issues" for the sourcing.
pub const MIN_REFRESH_INTERVAL_SECS: u64 = 120;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub refresh_interval_secs: u64,
    #[serde(default = "default_buttons")]
    pub buttons: Vec<ButtonAssignment>,
    #[serde(default)]
    pub budget: BudgetConfig,
    /// Whether Claude Deck should start automatically at login. Default
    /// true for new installs - button images live in volatile device
    /// memory (confirmed via mirajazz's API surface having no persist
    /// call), so nothing repaints them after a reboot unless something is
    /// running to do it. See ROADMAP.md "Known issues" for the writeup.
    #[serde(default = "default_true")]
    pub launch_at_login: bool,
}

/// No buttons assigned by default — the user picks explicitly which
/// buttons (if any) Claude Deck should own, since it can't know which
/// physical buttons are already in use by other software (e.g. AJAZZ
/// Stream Dock). Exposed as `pub` so "reset to defaults" can get back
/// here too.
pub fn default_buttons() -> Vec<ButtonAssignment> {
    Vec::new()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: 300,
            buttons: default_buttons(),
            budget: BudgetConfig::default(),
            launch_at_login: true,
        }
    }
}

fn config_path(app: &AppHandle) -> anyhow::Result<std::path::PathBuf> {
    let dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

pub fn load(app: &AppHandle) -> AppConfig {
    let mut cfg = (|| -> anyhow::Result<AppConfig> {
        let path = config_path(app)?;
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    })()
    .unwrap_or_default();

    // Auto-heal a too-low saved interval (e.g. from before this floor
    // existed) rather than leaving the user stuck re-triggering 429s
    // every launch.
    if cfg.refresh_interval_secs < MIN_REFRESH_INTERVAL_SECS {
        cfg.refresh_interval_secs = MIN_REFRESH_INTERVAL_SECS;
    }

    cfg
}

pub fn save(app: &AppHandle, config: &AppConfig) -> anyhow::Result<()> {
    let path = config_path(app)?;
    std::fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}
