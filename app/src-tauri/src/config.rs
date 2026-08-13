//! Local app config, stored as JSON under the OS app-data dir. No cloud
//! sync, no telemetry (SPEC.md §7).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub refresh_interval_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: 300,
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
