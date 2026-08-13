mod config;
mod device;
mod usage;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

struct AppState {
    latest_usage: Arc<Mutex<Option<usage::UsageSnapshot>>>,
}

#[tauri::command]
async fn get_usage_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<Option<usage::UsageSnapshot>, String> {
    Ok(state.latest_usage.lock().await.clone())
}

#[tauri::command]
async fn refresh_usage_now(
    state: tauri::State<'_, AppState>,
) -> Result<usage::UsageSnapshot, String> {
    let client = reqwest::Client::new();
    let snapshot = usage::poll_once(&client).await.map_err(|e| e.to_string())?;
    *state.latest_usage.lock().await = Some(snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn list_devices() -> Result<Vec<device::DiscoveredDevice>, String> {
    device::discover().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn push_test_pattern() -> Result<String, String> {
    device::push_test_pattern().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_button_events(timeout_secs: u64) -> Result<Vec<device::ButtonEvent>, String> {
    device::read_events_once(std::time::Duration::from_secs(timeout_secs))
        .await
        .map_err(|e| e.to_string())
}

/// Saves the new interval; takes effect after restart. Live-restarting the
/// running poll loop is a small follow-up, not done yet.
#[tauri::command]
async fn set_refresh_interval(app: tauri::AppHandle, seconds: u64) -> Result<(), String> {
    let cfg = config::AppConfig {
        refresh_interval_secs: seconds,
    };
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

fn spawn_usage_poller(app: &tauri::AppHandle) {
    let app = app.clone();
    let latest_usage = app.state::<AppState>().latest_usage.clone();

    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let config = config::load(&app);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            config.refresh_interval_secs,
        ));

        loop {
            interval.tick().await;

            match usage::poll_once(&client).await {
                Ok(snapshot) => {
                    *latest_usage.lock().await = Some(snapshot.clone());
                    let _ = app.emit("usage://updated", &snapshot);
                }
                Err(err) => {
                    eprintln!("usage poll failed: {err:#}");
                    let _ = app.emit("usage://error", err.to_string());
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            latest_usage: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            get_usage_snapshot,
            refresh_usage_now,
            list_devices,
            push_test_pattern,
            read_button_events,
            set_refresh_interval,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Fetch once immediately on startup so the UI has data without
            // waiting for the first interval tick.
            let startup_state = handle.state::<AppState>().latest_usage.clone();
            let startup_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let client = reqwest::Client::new();
                match usage::poll_once(&client).await {
                    Ok(snapshot) => {
                        *startup_state.lock().await = Some(snapshot.clone());
                        let _ = startup_handle.emit("usage://updated", &snapshot);
                    }
                    Err(err) => {
                        eprintln!("initial usage poll failed: {err:#}");
                        let _ = startup_handle.emit("usage://error", err.to_string());
                    }
                }
            });

            spawn_usage_poller(&handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
