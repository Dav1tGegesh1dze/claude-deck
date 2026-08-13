//! Self-imposed daily soft budget (SPEC.md §6.3). This is **not** an
//! Anthropic-enforced limit — Anthropic only has session (5h) and weekly
//! (7d) limits. This is a locally-computed, user-configured convenience:
//! "warn me if I burn more than X percentage-points of my weekly budget in
//! one day," estimated from the weekly percent's delta since local
//! midnight.
//!
//! Known limitation: if the weekly window resets partway through the day,
//! the delta briefly reads as negative (clamped to 0) and under-counts
//! usage for the rest of that day. Acceptable for a soft/local heuristic —
//! not worth the complexity of detecting reset events to fix.

use crate::{config, usage};
use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BudgetState {
    day: String,
    weekly_percent_at_day_start: f64,
}

fn state_path(app: &AppHandle) -> anyhow::Result<std::path::PathBuf> {
    let dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("budget_state.json"))
}

fn load_state(app: &AppHandle) -> Option<BudgetState> {
    let path = state_path(app).ok()?;
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_state(app: &AppHandle, state: &BudgetState) {
    if let Ok(path) = state_path(app) {
        let _ = std::fs::write(
            path,
            serde_json::to_string_pretty(state).unwrap_or_default(),
        );
    }
}

/// Computes today's soft-budget usage as a [usage::LimitEntry] so it can be
/// rendered with the same `render::render_percent` path as the real
/// session/weekly metrics.
pub fn compute(
    app: &AppHandle,
    cfg: &config::BudgetConfig,
    weekly_percent: f64,
) -> usage::LimitEntry {
    let today = Local::now().format("%Y-%m-%d").to_string();

    let state = match load_state(app) {
        Some(s) if s.day == today => s,
        _ => {
            let fresh = BudgetState {
                day: today,
                weekly_percent_at_day_start: weekly_percent,
            };
            save_state(app, &fresh);
            fresh
        }
    };

    let used_today = (weekly_percent - state.weekly_percent_at_day_start).max(0.0);
    let cap = cfg.daily_cap_percent.max(0.001);
    let fraction = used_today / cap;

    let severity = if fraction >= 1.0 {
        "critical"
    } else if fraction >= 0.75 {
        "warning"
    } else {
        "normal"
    };

    usage::LimitEntry {
        kind: "budget".to_string(),
        percent: (fraction * 100.0).min(999.0),
        severity: severity.to_string(),
        resets_at: None,
        is_active: cfg.enabled,
    }
}
