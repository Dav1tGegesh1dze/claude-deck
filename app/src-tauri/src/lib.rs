mod config;
mod device;
mod render;
mod usage;

use mirajazz::device::Device;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tokio::sync::Mutex;

struct ConnectedDevice {
    device: Device,
    kind: device::Kind,
}

struct AppState {
    latest_usage: Arc<Mutex<Option<usage::UsageSnapshot>>>,
    device: Arc<Mutex<Option<ConnectedDevice>>>,
}

#[tauri::command]
async fn get_usage_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<Option<usage::UsageSnapshot>, String> {
    Ok(state.latest_usage.lock().await.clone())
}

#[tauri::command]
async fn refresh_usage_now(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<usage::UsageSnapshot, String> {
    let client = reqwest::Client::new();
    let snapshot = usage::poll_once(&client).await.map_err(|e| e.to_string())?;
    *state.latest_usage.lock().await = Some(snapshot.clone());
    apply_snapshot(&app, &snapshot).await;
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

fn session_and_weekly(
    snapshot: &usage::UsageSnapshot,
) -> (Option<&usage::LimitEntry>, Option<&usage::LimitEntry>) {
    let session = snapshot.limits.iter().find(|l| l.kind == "session");
    let weekly = snapshot.limits.iter().find(|l| l.kind == "weekly_all");
    (session, weekly)
}

/// Pushes session/weekly to the physical device (buttons 0/1) if one is
/// connected (auto-connecting on first use), and updates the tray tooltip.
/// Best-effort: a missing device or a push failure is logged, not fatal —
/// the app is still useful via its own window either way.
async fn apply_snapshot(app: &tauri::AppHandle, snapshot: &usage::UsageSnapshot) {
    update_tray_tooltip(app, snapshot);

    let state = app.state::<AppState>();
    let mut guard = state.device.lock().await;

    if guard.is_none() {
        match device::connect_first().await {
            Ok((device, kind)) => {
                println!("Connected to device: {}", kind.human_name());
                *guard = Some(ConnectedDevice { device, kind });
            }
            Err(_) => return, // no supported device plugged in right now
        }
    }

    let push_result = if let Some(conn) = guard.as_ref() {
        push_snapshot_to_device(conn, snapshot).await
    } else {
        return;
    };

    if let Err(e) = push_result {
        eprintln!("failed to push usage to device, will reconnect next poll: {e:#}");
        *guard = None;
    }
}

async fn push_snapshot_to_device(
    conn: &ConnectedDevice,
    snapshot: &usage::UsageSnapshot,
) -> anyhow::Result<()> {
    let (w, h) = conn.kind.image_format().size;
    let (session, weekly) = session_and_weekly(snapshot);

    if let Some(session) = session {
        let img = render::render_percent(session.percent, &session.severity, w as u32, h as u32);
        device::push_image(&conn.device, conn.kind, 0, img).await?;
    }

    if let Some(weekly) = weekly {
        let img = render::render_percent(weekly.percent, &weekly.severity, w as u32, h as u32);
        device::push_image(&conn.device, conn.kind, 1, img).await?;
    }

    Ok(())
}

fn update_tray_tooltip(app: &tauri::AppHandle, snapshot: &usage::UsageSnapshot) {
    let (session, weekly) = session_and_weekly(snapshot);
    let text = format!(
        "Claude Deck\nSession: {}\nWeekly: {}",
        session
            .map(|l| format!("{}%", l.percent.round() as i64))
            .unwrap_or_else(|| "—".to_string()),
        weekly
            .map(|l| format!("{}%", l.percent.round() as i64))
            .unwrap_or_else(|| "—".to_string()),
    );

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text));
    }
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
                    apply_snapshot(&app, &snapshot).await;
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
            device: Arc::new(Mutex::new(None)),
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

            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;

            TrayIconBuilder::with_id("main")
                .menu(&menu)
                .tooltip("Claude Deck")
                .on_menu_event(|app, event| {
                    if event.id() == "quit" {
                        app.exit(0);
                    }
                })
                .build(app)?;

            // Fetch once immediately on startup so the UI (and device, and
            // tray) have data without waiting for the first interval tick.
            let startup_state = handle.state::<AppState>().latest_usage.clone();
            let startup_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let client = reqwest::Client::new();
                match usage::poll_once(&client).await {
                    Ok(snapshot) => {
                        *startup_state.lock().await = Some(snapshot.clone());
                        let _ = startup_handle.emit("usage://updated", &snapshot);
                        apply_snapshot(&startup_handle, &snapshot).await;
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
