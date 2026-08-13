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
    pub icon_path: Option<String>,
}

impl Default for ButtonAssignment {
    fn default() -> Self {
        Self {
            metric: Metric::None,
            icon_path: None,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub refresh_interval_secs: u64,
    #[serde(default = "default_buttons")]
    pub buttons: Vec<ButtonAssignment>,
    #[serde(default)]
    pub budget: BudgetConfig,
}

fn default_buttons() -> Vec<ButtonAssignment> {
    vec![
        ButtonAssignment {
            metric: Metric::Session,
            icon_path: None,
        },
        ButtonAssignment {
            metric: Metric::Weekly,
            icon_path: None,
        },
    ]
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: 300,
            buttons: default_buttons(),
            budget: BudgetConfig::default(),
        }
    }
}

fn config_path(app: &AppHandle) -> anyhow::Result<std::path::PathBuf> {
    let dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

pub fn load(app: &AppHandle) -> AppConfig {
    (|| -> anyhow::Result<AppConfig> {
        let path = config_path(app)?;
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    })()
    .unwrap_or_default()
}

pub fn save(app: &AppHandle, config: &AppConfig) -> anyhow::Result<()> {
    let path = config_path(app)?;
    std::fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}
