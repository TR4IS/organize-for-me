//! Organize for Me: the window, the tray and the commands the screens call.
#![windows_subsystem = "windows"]

mod engine;
mod store;

use engine::{Engine, FolderChoice, LogLine, Preview, Snapshot};
use ledger_core::judge::Options;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;

const WINDOW_WIDTH: f64 = 328.0;
const TICK: Duration = Duration::from_millis(500);
const SHOW_FALLBACK: Duration = Duration::from_secs(5);
const REPO_URL: &str = "https://github.com/TR4IS/FileOrganizer";

struct Shared(Mutex<Engine>);

struct TrayText {
    open: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Loaded {
    state: Snapshot,
    choices: Vec<FolderChoice>,
    version: &'static str,
}

fn engine<'a>(shared: &'a State<Shared>) -> std::sync::MutexGuard<'a, Engine> {
    shared
        .0
        .lock()
        .expect("the engine lock is never held across a panic")
}

fn folder_choices(app: &AppHandle) -> Vec<FolderChoice> {
    let paths = app.path();
    [
        paths.download_dir(),
        paths.desktop_dir(),
        paths.document_dir(),
    ]
    .into_iter()
    .flatten()
    .map(|path| engine::folder_choice(&path.to_string_lossy()))
    .collect()
}

#[tauri::command]
fn load(app: AppHandle, shared: State<Shared>) -> Loaded {
    Loaded {
        state: engine(&shared).snapshot(),
        choices: folder_choices(&app),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
fn get_logs(shared: State<Shared>) -> Vec<LogLine> {
    engine(&shared).logs()
}

#[tauri::command]
fn save_config(cfg: Value, shared: State<Shared>, tray: State<TrayText>) {
    let mut engine = engine(&shared);
    engine.save_config(cfg);
    set_tray_language(&tray, &engine.language());
}

#[tauri::command]
fn start_watching(
    path: String,
    risk_accepted: bool,
    paused: bool,
    shared: State<Shared>,
) -> Result<Snapshot, String> {
    let mut engine = engine(&shared);
    engine.start_watching(&path, risk_accepted, paused)?;
    Ok(engine.snapshot())
}

#[tauri::command]
fn choose_folder(
    path: String,
    risk_accepted: bool,
    shared: State<Shared>,
) -> Result<Snapshot, String> {
    let mut engine = engine(&shared);
    engine.choose_folder(&path, risk_accepted)?;
    Ok(engine.snapshot())
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<FolderChoice> {
    let picked = app.dialog().file().blocking_pick_folder()?;
    let path: PathBuf = picked.into_path().ok()?;
    Some(engine::folder_choice(&path.to_string_lossy()))
}

#[tauri::command]
fn preview(path: String) -> Preview {
    engine::preview(&path, Options::default())
}

#[tauri::command]
fn set_paused(paused: bool, shared: State<Shared>) -> Snapshot {
    let mut engine = engine(&shared);
    engine.set_paused(paused);
    engine.snapshot()
}

#[tauri::command]
fn new_receipt(shared: State<Shared>) -> Snapshot {
    let mut engine = engine(&shared);
    engine.new_receipt();
    engine.snapshot()
}

#[tauri::command]
fn forget(shared: State<Shared>) -> Snapshot {
    let mut engine = engine(&shared);
    engine.forget();
    engine.snapshot()
}

#[tauri::command]
fn item_action(id: u64, action: String, bin: Option<String>, shared: State<Shared>) -> Snapshot {
    let mut engine = engine(&shared);
    match action.as_str() {
        "void" => engine.void_item(id),
        "redo" => engine.redo_item(id),
        "pick" => engine.pick_item(id, bin.as_deref().unwrap_or("")),
        "leave" => engine.leave_item(id),
        "retry" => engine.retry_item(id),
        _ => {}
    }
    engine.snapshot()
}

#[tauri::command]
fn undo_log(log_id: u64, shared: State<Shared>) -> bool {
    engine(&shared).undo_log(log_id)
}

/// Shows a file in Explorer. The path comes from the app's own records.
#[tauri::command]
fn show_item(id: u64, shared: State<Shared>) {
    use std::os::windows::process::CommandExt;
    let Some(path) = engine(&shared).item_path(id) else {
        return;
    };
    let text = path.to_string_lossy();
    let plain = text.strip_prefix(r"\\?\").unwrap_or(&text).replace('"', "");
    let _ = Command::new("explorer")
        .raw_arg(format!("/select,\"{plain}\""))
        .spawn();
}

#[tauri::command]
fn set_startup(on: bool, app: AppHandle, shared: State<Shared>) -> Result<Snapshot, String> {
    let autostart = app.autolaunch();
    let result = if on {
        autostart.enable()
    } else {
        autostart.disable()
    };
    result.map_err(|e| e.to_string())?;
    let mut engine = engine(&shared);
    engine.set_startup_setting(on);
    Ok(engine.snapshot())
}

#[tauri::command]
fn open_repo() {
    let _ = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", REPO_URL])
        .spawn();
}

#[tauri::command]
fn open_releases() {
    let _ = Command::new("rundll32")
        .args([
            "url.dll,FileProtocolHandler",
            &format!("{REPO_URL}/releases"),
        ])
        .spawn();
}

#[tauri::command]
fn set_window_height(height: f64, window: WebviewWindow) {
    let _ = window.set_size(LogicalSize::new(WINDOW_WIDTH, height));
    keep_on_screen(&window, height);
}

/// Moves the window up if its bottom edge would sit behind the taskbar or off the screen.
fn keep_on_screen(window: &WebviewWindow, height: f64) {
    let (Ok(Some(monitor)), Ok(position)) = (window.current_monitor(), window.outer_position())
    else {
        return;
    };
    let area = monitor.work_area();
    let physical_height = (height * monitor.scale_factor()).ceil() as i32;
    let bottom = area.position.y + area.size.height as i32;
    let y = (bottom - physical_height).max(area.position.y);
    if position.y > y {
        let _ = window.set_position(PhysicalPosition::new(position.x, y));
    }
}

/// Shows the window once the screens have drawn, so it never opens at the wrong size.
#[tauri::command]
fn show_window(app: AppHandle) {
    show_main(&app);
}

#[tauri::command]
fn start_drag(window: WebviewWindow) {
    let _ = window.start_dragging();
}

#[tauri::command]
fn close_window(app: AppHandle, window: WebviewWindow, shared: State<Shared>) {
    if engine(&shared).closes_to_tray() {
        let _ = window.hide();
    } else {
        app.exit(0);
    }
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn set_tray_language(tray: &TrayText, language: &str) {
    let (open, quit) = if language == "ar" {
        ("افتح", "قفّل التطبيق")
    } else {
        ("Open", "Quit")
    };
    let _ = tray.open.set_text(open);
    let _ = tray.quit.set_text(quit);
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    // Test runs use their own data, so they can never touch a real receipt.
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("OFM_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let engine = Engine::open(&data_dir(&handle)?).map_err(|error| {
        use tauri_plugin_dialog::MessageDialogKind;
        handle
            .dialog()
            .message(format!(
                "Organize for Me cannot open its records, so it will not move anything.\n\n{error}"
            ))
            .kind(MessageDialogKind::Error)
            .blocking_show();
        error
    })?;
    let language = engine.language();
    app.manage(Shared(Mutex::new(engine)));

    let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let tray_text = TrayText { open, quit };
    set_tray_language(&tray_text, &language);
    app.manage(tray_text);

    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Organize for Me")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    // If the screens ever fail to load, the window must still appear.
    let fallback = handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SHOW_FALLBACK);
        if let Some(window) = fallback.get_webview_window("main")
            && !window.is_visible().unwrap_or(true)
        {
            show_main(&fallback);
        }
    });

    let ticker = handle.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        loop {
            std::thread::sleep(TICK);
            let shared = ticker.state::<Shared>();
            let mut engine = shared
                .0
                .lock()
                .expect("the engine lock is never held across a panic");
            if engine.tick(started.elapsed().as_millis() as u64) {
                let snapshot = engine.snapshot();
                drop(engine);
                let _ = ticker.emit("changed", snapshot);
            }
        }
    });
    Ok(())
}

fn main() {
    tauri::Builder::default()
        // First, because two copies writing one journal would break its promises.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(setup)
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let shared = window.state::<Shared>();
                let to_tray = shared
                    .0
                    .lock()
                    .expect("the engine lock is never held across a panic")
                    .closes_to_tray();
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load,
            get_logs,
            save_config,
            start_watching,
            choose_folder,
            pick_folder,
            preview,
            set_paused,
            new_receipt,
            forget,
            item_action,
            undo_log,
            show_item,
            set_startup,
            open_repo,
            open_releases,
            set_window_height,
            show_window,
            start_drag,
            close_window
        ])
        .run(tauri::generate_context!())
        .expect("the window could not start");
}
