mod budget;
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
use tauri_plugin_dialog::DialogExt;
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
    let mut cfg = config::load(&app);
    cfg.refresh_interval_secs = seconds;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_config(app: tauri::AppHandle) -> Result<config::AppConfig, String> {
    Ok(config::load(&app))
}

#[tauri::command]
async fn set_button_metric(
    app: tauri::AppHandle,
    button_index: usize,
    metric: config::Metric,
) -> Result<(), String> {
    let mut cfg = config::load(&app);
    ensure_button_slot(&mut cfg, button_index);
    cfg.buttons[button_index].metric = metric;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_budget_config(
    app: tauri::AppHandle,
    enabled: bool,
    daily_cap_percent: f64,
) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.budget = config::BudgetConfig {
        enabled,
        daily_cap_percent,
    };
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

/// Opens a native file picker, copies the chosen image into the app's
/// config dir (so it survives the source file moving/being deleted), and
/// assigns it as the icon for `button_index`. Returns the stored path, or
/// `None` if the user cancelled.
#[tauri::command]
async fn pick_icon_for_button(
    app: tauri::AppHandle,
    button_index: usize,
) -> Result<Option<String>, String> {
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;

    let Some(picked) = picked else {
        return Ok(None);
    };

    let source = picked.into_path().map_err(|e| e.to_string())?;
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");

    let icons_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("icons");
    std::fs::create_dir_all(&icons_dir).map_err(|e| e.to_string())?;

    let dest = icons_dir.join(format!("button_{button_index}.{ext}"));
    std::fs::copy(&source, &dest).map_err(|e| e.to_string())?;
    let dest_str = dest.to_string_lossy().to_string();

    let mut cfg = config::load(&app);
    ensure_button_slot(&mut cfg, button_index);
    cfg.buttons[button_index].icon_path = Some(dest_str.clone());
    config::save(&app, &cfg).map_err(|e| e.to_string())?;

    Ok(Some(dest_str))
}

fn ensure_button_slot(cfg: &mut config::AppConfig, index: usize) {
    while cfg.buttons.len() <= index {
        cfg.buttons.push(config::ButtonAssignment::default());
    }
}

fn session_and_weekly(
    snapshot: &usage::UsageSnapshot,
) -> (Option<&usage::LimitEntry>, Option<&usage::LimitEntry>) {
    let session = snapshot.limits.iter().find(|l| l.kind == "session");
    let weekly = snapshot.limits.iter().find(|l| l.kind == "weekly_all");
    (session, weekly)
}

/// Pushes each configured button's metric to the physical device if one is
/// connected (auto-connecting on first use), and updates the tray tooltip.
/// Best-effort: a missing device or a push failure is logged, not fatal —
/// the app is still useful via its own window either way.
async fn apply_snapshot(app: &tauri::AppHandle, snapshot: &usage::UsageSnapshot) {
    update_tray_tooltip(app, snapshot);

    let cfg = config::load(app);
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
        push_snapshot_to_device(app, conn, snapshot, &cfg).await
    } else {
        return;
    };

    if let Err(e) = push_result {
        eprintln!("failed to push usage to device, will reconnect next poll: {e:#}");
        *guard = None;
    }
}

async fn push_snapshot_to_device(
    app: &tauri::AppHandle,
    conn: &ConnectedDevice,
    snapshot: &usage::UsageSnapshot,
    cfg: &config::AppConfig,
) -> anyhow::Result<()> {
    let (w, h) = conn.kind.image_format().size;
    let (session, weekly) = session_and_weekly(snapshot);
    let weekly_percent = weekly.map(|l| l.percent).unwrap_or(0.0);

    for (index, assignment) in cfg.buttons.iter().enumerate() {
        if index >= device::KEY_COUNT {
            break;
        }

        let entry: Option<usage::LimitEntry> = match assignment.metric {
            config::Metric::Session => session.cloned(),
            config::Metric::Weekly => weekly.cloned(),
            config::Metric::Budget if cfg.budget.enabled => {
                Some(budget::compute(app, &cfg.budget, weekly_percent))
            }
            config::Metric::Budget | config::Metric::None => None,
        };

        let Some(entry) = entry else { continue };

        let image = match &assignment.icon_path {
            Some(path) => render::render_percent_on_background(
                std::path::Path::new(path),
                entry.percent,
                &entry.severity,
                w as u32,
                h as u32,
            )?,
            None => render::render_percent(entry.percent, &entry.severity, w as u32, h as u32),
        };

        device::push_image(&conn.device, conn.kind, index as u8, image).await?;
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
        .plugin(tauri_plugin_dialog::init())
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
            get_config,
            set_button_metric,
            set_budget_config,
            pick_icon_for_button,
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
