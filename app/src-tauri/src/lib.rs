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
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Mutex;

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray/tray-icon@2x.png");
/// How many times to retry connecting to the device right after launch,
/// and how far apart. Found necessary 2026-08-14: right after a reboot
/// the app can start before the OS finishes USB enumeration, so a single
/// connect attempt at startup can miss a device that's plugged in and
/// about to be ready a few seconds later.
const STARTUP_CONNECT_ATTEMPTS: u32 = 8;
const STARTUP_CONNECT_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

struct ConnectedDevice {
    device: Device,
    kind: device::Kind,
}

struct AppState {
    latest_usage: Arc<Mutex<Option<usage::UsageSnapshot>>>,
    device: Arc<Mutex<Option<ConnectedDevice>>>,
    /// Cached OAuth token so we don't re-touch the OS credential store
    /// (macOS Keychain, which prompts) on every poll tick — see
    /// usage::poll_cached.
    cached_token: Arc<Mutex<Option<String>>>,
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
    let snapshot = usage::poll_cached(&client, &state.cached_token)
        .await
        .map_err(|e| e.to_string())?;
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
/// running poll loop is a small follow-up, not done yet. Clamped to
/// config::MIN_REFRESH_INTERVAL_SECS - see that constant's doc comment.
#[tauri::command]
async fn set_refresh_interval(app: tauri::AppHandle, seconds: u64) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.refresh_interval_secs = seconds.max(config::MIN_REFRESH_INTERVAL_SECS);
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
    require_screen_button(button_index)?;
    let old_cfg = config::load(&app);
    let mut cfg = old_cfg.clone();
    ensure_button_slot(&mut cfg, button_index);
    cfg.buttons[button_index].metric = metric;
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    clear_released_buttons(&app, &old_cfg, &cfg).await;
    push_now(&app).await;
    Ok(())
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
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    push_now(&app).await;
    Ok(())
}

/// Resets every button back to `none` (nothing assigned), and actively
/// blanks the screen on any button that was previously assigned — see
/// clear_released_buttons. Cannot restore whatever image another app
/// (e.g. AJAZZ's Stream Dock) had on a button before Claude Deck painted
/// over it — these HID displays are write-only, there's no way to read
/// back or recall a *previous* image, only blank the current one. The
/// other app still has to repaint its own icon itself (switch
/// pages/profiles in it, or unplug/replug the device).
#[tauri::command]
async fn reset_button_assignments(app: tauri::AppHandle) -> Result<(), String> {
    let old_cfg = config::load(&app);
    let mut cfg = old_cfg.clone();
    cfg.buttons = config::default_buttons();
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    clear_released_buttons(&app, &old_cfg, &cfg).await;
    push_now(&app).await;
    Ok(())
}

#[tauri::command]
async fn get_launch_at_login(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_launch_at_login(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    result.map_err(|e| e.to_string())?;

    let mut cfg = config::load(&app);
    cfg.launch_at_login = enabled;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

/// For any screen button that was assigned a metric in `old` but is
/// `none` in `new`, actively blanks that button's screen via
/// `Device::clear_button_image` instead of just leaving the last-rendered
/// gauge stuck there — "unassigning" a button should look unassigned.
/// Best-effort: silently does nothing if no device is connected.
async fn clear_released_buttons(
    app: &tauri::AppHandle,
    old: &config::AppConfig,
    new: &config::AppConfig,
) {
    let state = app.state::<AppState>();
    let guard = state.device.lock().await;
    let Some(conn) = guard.as_ref() else {
        return;
    };

    let mut cleared_any = false;

    for i in 0..device::SCREEN_KEY_COUNT {
        let was = old
            .buttons
            .get(i)
            .map(|b| b.metric)
            .unwrap_or(config::Metric::None);
        let now = new
            .buttons
            .get(i)
            .map(|b| b.metric)
            .unwrap_or(config::Metric::None);

        if was != config::Metric::None && now == config::Metric::None {
            if let Err(e) = conn.device.clear_button_image(i as u8).await {
                log::error!("failed to clear button {i}: {e}");
            } else {
                cleared_any = true;
            }
        }
    }

    if cleared_any {
        if let Err(e) = conn.device.flush().await {
            log::error!("failed to flush after clearing buttons: {e}");
        }
    }
}

/// Blanks every currently-assigned button's screen — used when the app is
/// about to quit, so the physical device honestly shows "not running"
/// instead of silently freezing on stale numbers. Best-effort: does
/// nothing if no device is connected.
async fn blank_assigned_buttons(app: &tauri::AppHandle) {
    let cfg = config::load(app);
    let state = app.state::<AppState>();
    let guard = state.device.lock().await;
    let Some(conn) = guard.as_ref() else {
        return;
    };

    let mut cleared_any = false;

    for (i, assignment) in cfg.buttons.iter().enumerate() {
        if i >= device::SCREEN_KEY_COUNT {
            break;
        }
        if assignment.metric != config::Metric::None {
            match conn.device.clear_button_image(i as u8).await {
                Ok(()) => cleared_any = true,
                Err(e) => log::error!("failed to clear button {i} on quit: {e}"),
            }
        }
    }

    if cleared_any {
        if let Err(e) = conn.device.flush().await {
            log::error!("failed to flush after clearing buttons on quit: {e}");
        }
    }
}

fn ensure_button_slot(cfg: &mut config::AppConfig, index: usize) {
    while cfg.buttons.len() <= index {
        cfg.buttons.push(config::ButtonAssignment::default());
    }
}

/// Buttons 6/7/8 on the AKP03 family have no screen (SCREEN_KEY_COUNT=6) —
/// reject assigning them here too, not just at push time, so a bad index
/// fails loudly instead of just silently never showing anything.
fn require_screen_button(index: usize) -> Result<(), String> {
    if index >= device::SCREEN_KEY_COUNT {
        return Err(format!(
            "button {index} has no screen (only buttons 0-{} do)",
            device::SCREEN_KEY_COUNT - 1
        ));
    }
    Ok(())
}

/// Re-pushes the last known usage snapshot to the device immediately,
/// instead of making the user wait for the next poll tick (up to
/// `refresh_interval_secs`) to see a settings change take effect. No-op if
/// no snapshot has been fetched yet.
async fn push_now(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let snapshot = state.latest_usage.lock().await.clone();
    if let Some(snapshot) = snapshot {
        apply_snapshot(app, &snapshot).await;
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
                log::info!("Connected to device: {}", kind.human_name());
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
        log::error!("failed to push usage to device, will reconnect next poll: {e:#}");
        *guard = None;
    }
}

/// Short label shown above the percentage so buttons showing different
/// metrics are visually distinguishable (hardware testing showed this was
/// missing — session/weekly looked identical at a glance).
fn metric_label(metric: config::Metric) -> &'static str {
    match metric {
        config::Metric::Session => "5H",
        config::Metric::Weekly => "7D",
        config::Metric::Budget => "BUD",
        config::Metric::None => "",
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
        if index >= device::SCREEN_KEY_COUNT {
            break; // buttons past this have no screen - nothing to push
        }

        let label = metric_label(assignment.metric);

        // Budget assigned but tracking disabled: show a clear "off" state
        // instead of silently pushing nothing, which just looked broken.
        if assignment.metric == config::Metric::Budget && !cfg.budget.enabled {
            let image = render::render_disabled(label, w as u32, h as u32);
            device::push_image(&conn.device, conn.kind, index as u8, image).await?;
            continue;
        }

        let entry: Option<usage::LimitEntry> = match assignment.metric {
            config::Metric::Session => session.cloned(),
            config::Metric::Weekly => weekly.cloned(),
            config::Metric::Budget => Some(budget::compute(app, &cfg.budget, weekly_percent)),
            config::Metric::None => None,
        };

        let Some(entry) = entry else { continue };

        let image =
            render::render_percent(label, entry.percent, &entry.severity, w as u32, h as u32);

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

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn spawn_usage_poller(app: &tauri::AppHandle) {
    let app = app.clone();
    let state = app.state::<AppState>();
    let latest_usage = state.latest_usage.clone();
    let cached_token = state.cached_token.clone();

    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let config = config::load(&app);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            config.refresh_interval_secs,
        ));

        loop {
            interval.tick().await;

            match usage::poll_cached(&client, &cached_token).await {
                Ok(snapshot) => {
                    *latest_usage.lock().await = Some(snapshot.clone());
                    let _ = app.emit("usage://updated", &snapshot);
                    apply_snapshot(&app, &snapshot).await;
                }
                Err(usage::PollError::RateLimited { retry_after_secs }) => {
                    let backoff = retry_after_secs.unwrap_or(120);
                    log::warn!("usage endpoint rate limited (429), backing off {backoff}s");
                    let _ = app.emit(
                        "usage://error",
                        format!("Rate limited by usage endpoint - waiting {backoff}s"),
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                }
                Err(err) => {
                    log::error!("usage poll failed: {err:#}");
                    let _ = app.emit("usage://error", err.to_string());
                }
            }
        }
    });
}

/// Retries connecting to the device a few times right after launch — see
/// STARTUP_CONNECT_ATTEMPTS's doc comment for why a single attempt isn't
/// enough right after a reboot. Backs off immediately (checking
/// AppState.device first) if a poll tick elsewhere already connected.
fn spawn_startup_device_connect(app: &tauri::AppHandle) {
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();

        for attempt in 0..STARTUP_CONNECT_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(STARTUP_CONNECT_RETRY_DELAY).await;
            }

            {
                let guard = state.device.lock().await;
                if guard.is_some() {
                    return; // a usage poll tick already connected one
                }
            }

            if let Ok((device, kind)) = device::connect_first().await {
                log::info!(
                    "Connected to device on startup (attempt {}): {}",
                    attempt + 1,
                    kind.human_name()
                );
                *state.device.lock().await = Some(ConnectedDevice { device, kind });
                push_now(&app).await;
                return;
            }
        }

        log::info!("No supported device found after startup connection retries");
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(AppState {
            latest_usage: Arc::new(Mutex::new(None)),
            device: Arc::new(Mutex::new(None)),
            cached_token: Arc::new(Mutex::new(None)),
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
            reset_button_assignments,
            get_launch_at_login,
            set_launch_at_login,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Sync the OS-level login-item registration to match the
            // saved preference (default true - see config::AppConfig).
            // Best-effort: platforms/sandboxes that reject this shouldn't
            // block startup.
            let cfg = config::load(&handle);
            let autolaunch = handle.autolaunch();
            let sync_result = if cfg.launch_at_login {
                autolaunch.enable()
            } else {
                autolaunch.disable()
            };
            if let Err(e) = sync_result {
                log::warn!("failed to sync launch-at-login state: {e}");
            }

            let tray_icon = tauri::image::Image::from_bytes(TRAY_ICON_BYTES)?;

            let show_item = MenuItem::with_id(app, "show", "Show Claude Deck", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .tooltip("Claude Deck")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "quit" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            blank_assigned_buttons(&handle).await;
                            handle.exit(0);
                        });
                    }
                    "show" => show_main_window(app),
                    _ => {}
                })
                .build(app)?;

            // Closing the window should not quit the app - the whole point
            // is that it keeps polling and updating the physical device in
            // the background (found missing during hardware testing: the
            // display just froze on close because the process was exiting
            // and killing the poller with it). Hide instead; "Quit" in the
            // tray menu (or a confirmed Cmd+Q/Dock-Quit, see .run() below)
            // is the only way to actually exit.
            if let Some(window) = app.get_webview_window("main") {
                let window_to_hide = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_to_hide.hide();
                    }
                });
            }

            // Fetch once immediately on startup so the UI (and device, and
            // tray) have data without waiting for the first interval tick.
            let startup_state = handle.state::<AppState>().latest_usage.clone();
            let startup_token = handle.state::<AppState>().cached_token.clone();
            let startup_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let client = reqwest::Client::new();
                match usage::poll_cached(&client, &startup_token).await {
                    Ok(snapshot) => {
                        *startup_state.lock().await = Some(snapshot.clone());
                        let _ = startup_handle.emit("usage://updated", &snapshot);
                        apply_snapshot(&startup_handle, &snapshot).await;
                    }
                    Err(err) => {
                        log::error!("initial usage poll failed: {err:#}");
                        let _ = startup_handle.emit("usage://error", err.to_string());
                    }
                }
            });

            spawn_startup_device_connect(&handle);
            spawn_usage_poller(&handle);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // macOS: clicking the Dock icon while the window is hidden
            // should bring it back, matching normal Mac app behavior.
            // RunEvent::Reopen only exists on macOS - this must be cfg'd
            // out on other platforms or the build doesn't even compile
            // (found via a real Windows CI failure, not guessed). Match on
            // a reference so `event` is still available below.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &event {
                show_main_window(app_handle);
            }

            // Cmd+Q / Dock right-click "Quit" bypass the window-close
            // handling above entirely (different event, see SPEC.md
            // "Process lifecycle"). `code: None` means the OS/user
            // triggered this directly, as opposed to our own tray "Quit"
            // item calling app.exit(0) (which reports code: Some(0) here
            // and is intentionally left alone). Confirm first, since
            // quitting stops the background usage updates - decided
            // 2026-08-14 after the reboot-persistence investigation.
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
                let handle = app_handle.clone();
                app_handle
                    .dialog()
                    .message("Quitting stops Claude Deck from updating your device.")
                    .title("Quit Claude Deck?")
                    .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
                    .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancelCustom(
                        "Quit".to_string(),
                        "Cancel".to_string(),
                    ))
                    .show(move |confirmed| {
                        if confirmed {
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                blank_assigned_buttons(&handle).await;
                                handle.exit(0);
                            });
                        }
                    });
            }
        });
}
