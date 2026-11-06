//! Organize for Me: the window, the tray and the commands the screens call.
#![windows_subsystem = "windows"]

mod chrome_host;
mod engine;
mod store;

use engine::{Engine, FolderChoice, LogLine, Preview, Snapshot};
use ledger_core::judge::Options;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
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
/// Matches `identifier` in tauri.conf.json, which names the app's data folder.
const APP_ID: &str = "com.tr4is.organizeforme";
const CRASH_LOG: &str = "crash.log";
const CRASH_SEEN: &str = "crash.seen";
const REPO_URL: &str = "https://github.com/TR4IS/organize-for-me";

struct Shared(Mutex<Engine>);

/// The close button's setting, kept apart from the engine so the window can read it without waiting.
struct CloseToTray(AtomicBool);

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

// The window's thread must never wait for the engine. The engine can be busy for a while, reading a big folder or
// sorting a burst of new files while antivirus checks each one, and a window that waits for it shows as
// "Not Responding". So every command that touches the engine runs on a worker thread, and the window stays free.
async fn off_window<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
}

async fn with_engine<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&mut Engine) -> T + Send + 'static,
) -> Result<T, String> {
    off_window(move || {
        let shared = app.state::<Shared>();
        let mut engine = engine(&shared);
        work(&mut engine)
    })
    .await
}

#[tauri::command]
async fn load(app: AppHandle) -> Result<Loaded, String> {
    let choices = folder_choices(&app);
    with_engine(app, move |engine| Loaded {
        state: engine.snapshot(),
        choices,
        version: env!("CARGO_PKG_VERSION"),
    })
    .await
}

#[tauri::command]
async fn get_logs(app: AppHandle) -> Result<Vec<LogLine>, String> {
    with_engine(app, |engine| engine.logs()).await
}

#[tauri::command]
async fn save_config(cfg: Value, app: AppHandle) -> Result<(), String> {
    let (language, to_tray) = with_engine(app.clone(), move |engine| {
        engine.save_config(cfg);
        (engine.language(), engine.closes_to_tray())
    })
    .await?;
    app.state::<CloseToTray>()
        .0
        .store(to_tray, Ordering::Relaxed);
    set_tray_language(&app.state::<TrayText>(), &language);
    Ok(())
}

#[tauri::command]
async fn start_watching(
    path: String,
    risk_accepted: bool,
    paused: bool,
    app: AppHandle,
) -> Result<Snapshot, String> {
    with_engine(app, move |engine| {
        engine.start_watching(&path, risk_accepted, paused)?;
        Ok(engine.snapshot())
    })
    .await?
}

#[tauri::command]
async fn choose_folder(
    path: String,
    risk_accepted: bool,
    app: AppHandle,
) -> Result<Snapshot, String> {
    with_engine(app, move |engine| {
        engine.choose_folder(&path, risk_accepted)?;
        Ok(engine.snapshot())
    })
    .await?
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<FolderChoice> {
    let picked = app.dialog().file().blocking_pick_folder()?;
    let path: PathBuf = picked.into_path().ok()?;
    Some(engine::folder_choice(&path.to_string_lossy()))
}

#[tauri::command]
async fn preview(path: String) -> Preview {
    off_window(move || engine::preview(&path, Options::default()))
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn set_paused(paused: bool, app: AppHandle) -> Result<Snapshot, String> {
    with_engine(app, move |engine| {
        engine.set_paused(paused);
        engine.snapshot()
    })
    .await
}

#[tauri::command]
async fn new_receipt(app: AppHandle) -> Result<Snapshot, String> {
    with_engine(app, |engine| {
        engine.new_receipt();
        engine.snapshot()
    })
    .await
}

#[tauri::command]
async fn forget(app: AppHandle) -> Result<Snapshot, String> {
    with_engine(app, |engine| {
        engine.forget();
        engine.snapshot()
    })
    .await
}

#[tauri::command]
async fn item_action(
    id: u64,
    action: String,
    bin: Option<String>,
    app: AppHandle,
) -> Result<Snapshot, String> {
    with_engine(app, move |engine| {
        match action.as_str() {
            "void" => engine.void_item(id),
            "redo" => engine.redo_item(id),
            "pick" => engine.pick_item(id, bin.as_deref().unwrap_or("")),
            "leave" => engine.leave_item(id),
            "retry" => engine.retry_item(id),
            _ => {}
        }
        engine.snapshot()
    })
    .await
}

#[tauri::command]
async fn undo_log(log_id: u64, app: AppHandle) -> Result<bool, String> {
    with_engine(app, move |engine| engine.undo_log(log_id)).await
}

/// Shows a file in Explorer. The path comes from the app's own records.
#[tauri::command]
async fn show_item(id: u64, app: AppHandle) {
    use std::os::windows::process::CommandExt;
    let Ok(Some(path)) = with_engine(app, move |engine| engine.item_path(id)).await else {
        return;
    };
    let text = path.to_string_lossy();
    let plain = text.strip_prefix(r"\\?\").unwrap_or(&text).replace('"', "");
    let _ = Command::new("explorer")
        .raw_arg(format!("/select,\"{plain}\""))
        .spawn();
}

#[tauri::command]
async fn set_startup(on: bool, app: AppHandle) -> Result<Snapshot, String> {
    let autostart = app.autolaunch();
    let result = if on {
        autostart.enable()
    } else {
        autostart.disable()
    };
    result.map_err(|e| e.to_string())?;
    with_engine(app, move |engine| {
        engine.set_startup_setting(on);
        engine.snapshot()
    })
    .await
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
fn close_window(app: AppHandle, window: WebviewWindow, to_tray: State<CloseToTray>) {
    if to_tray.0.load(Ordering::Relaxed) {
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
    let dir = data_dir(&handle)?;
    let mut engine = Engine::open(&dir).map_err(|error| {
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
    if new_crash(&dir) {
        engine.note_crash(CRASH_LOG);
    }
    let language = engine.language();
    app.manage(CloseToTray(AtomicBool::new(engine.closes_to_tray())));
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

/// True once per crash written to crash.log since the last start. Both files only ever grow: crash.seen gets one
/// line for each crash already reported.
fn new_crash(dir: &std::path::Path) -> bool {
    use std::io::Write;
    let lines = |name| {
        std::fs::read_to_string(dir.join(name)).map_or(0, |text: String| text.lines().count())
    };
    let crashes = lines(CRASH_LOG);
    let seen = lines(CRASH_SEEN);
    if crashes <= seen {
        return false;
    }
    if let Ok(mut out) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(CRASH_SEEN))
    {
        let _ = out.write_all(
            "seen
"
            .repeat(crashes - seen)
            .as_bytes(),
        );
    }
    true
}

/// Where the records live: the app's data folder, or in test runs the folder they name.
fn records_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("OFM_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join(APP_ID))
}

/// The app aborts on a panic, which leaves no trace. This writes the reason to crash.log first.
fn log_crashes() {
    let Some(dir) = records_dir() else {
        return;
    };
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut out) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(CRASH_LOG))
        {
            let _ = writeln!(out, "{seconds} v{} {info}", env!("CARGO_PKG_VERSION"));
        }
    }));
}

/// The app's language, read without changing anything. English when it cannot be read.
fn saved_language(dir: Option<&std::path::Path>) -> String {
    dir.and_then(|dir| std::fs::read_to_string(dir.join("state.json")).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|state| state["cfg"]["lang"].as_str().map(String::from))
        .unwrap_or_else(|| "en".into())
}

fn main() {
    log_crashes();
    if chrome_host::started_by_chrome() {
        let records = records_dir();
        let language = saved_language(records.as_deref());
        chrome_host::serve(records, language);
        return;
    }
    // Lets a test run check that a crash is written down and reported.
    #[cfg(debug_assertions)]
    if std::env::var_os("OFM_TEST_CRASH").is_some() {
        panic!("test crash");
    }
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
                if window.state::<CloseToTray>().0.load(Ordering::Relaxed) {
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
