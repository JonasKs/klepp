#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod config;
mod glass;
mod paste;
mod store;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::http::{Response, StatusCode};
use tauri::{
    ActivationPolicy, AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State,
    WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use clipboard::{NewClip, Watcher};
use config::Config;
use store::{Kind, Store};

/// Panel geometry in logical pixels.
const PANEL_HEIGHT: f64 = 340.0;
const PANEL_MARGIN: f64 = 10.0;
const PANEL_RADIUS: f64 = 22.0;
/// How often the retention job runs.
const RETENTION_INTERVAL: Duration = Duration::from_secs(30 * 60);

pub struct AppState {
    pub store: Mutex<Store>,
    pub watcher: Arc<Watcher>,
    /// When set (from the menu bar), new clips are not recorded.
    pub paused: AtomicBool,
}

#[derive(serde::Serialize)]
struct ClipView {
    id: i64,
    ts: String,
    kind: &'static str,
    text: String,
    width: u32,
    height: u32,
    app: Option<String>,
    len: usize,
}

#[tauri::command]
fn list(query: String, state: State<AppState>) -> Vec<ClipView> {
    let store = state.store.lock().unwrap();
    store
        .search(&query)
        .into_iter()
        .map(|c| ClipView {
            id: c.id,
            ts: c.ts.to_rfc3339(),
            kind: if c.kind == Kind::Image {
                "image"
            } else {
                "text"
            },
            // Cap what we ship to the UI; the full text stays in the database.
            text: c.text.chars().take(4000).collect(),
            width: c.width,
            height: c.height,
            app: c.app.clone(),
            len: c.text.chars().count(),
        })
        .collect()
}

#[tauri::command]
fn remove(id: i64, state: State<AppState>) -> bool {
    state.store.lock().unwrap().delete(id)
}

/// Put the clip on the clipboard, close the panel and paste into the
/// previously active app.
#[tauri::command]
fn pick(id: i64, app: AppHandle, state: State<AppState>) -> bool {
    let (clip, image) = {
        let store = state.store.lock().unwrap();
        let Some(clip) = store.get(id) else {
            return false;
        };
        let image = (clip.kind == Kind::Image)
            .then(|| store.image(id))
            .flatten();
        (clip, image)
    };
    match image {
        Some(png) => state.watcher.set_image(&png),
        None => state.watcher.set_text(&clip.text),
    }
    hide_panel(&app);
    thread::spawn(|| {
        // Give macOS a moment to hand focus back to the previous app.
        thread::sleep(Duration::from_millis(150));
        paste::send_cmd_v();
    });
    true
}

#[tauri::command]
fn hide(app: AppHandle) {
    hide_panel(&app);
}

pub fn show_panel(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };

    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());

    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let wa = m.work_area();
        let margin = (PANEL_MARGIN * scale).round() as i32;
        let height = (PANEL_HEIGHT * scale).round() as u32;
        let width = wa.size.width.saturating_sub(2 * margin as u32);
        let x = wa.position.x + margin;
        let y = wa.position.y + wa.size.height as i32 - height as i32 - margin;
        let _ = win.set_size(PhysicalSize::new(width, height));
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }

    let _ = app.show();
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit("klepp://shown", ());
}

fn hide_panel(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
    // Hiding the app returns focus to whatever was active before.
    let _ = app.hide();
}

fn toggle_panel(app: &AppHandle) {
    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_panel(app);
    } else {
        show_panel(app);
    }
}

/// (Re)load `config.toml`, push it to the watcher and bind the shortcut.
/// Returns the active shortcut.
pub fn apply_config(app: &AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let root = state.store.lock().unwrap().root().to_path_buf();
    let cfg = Config::load(&root)?;
    let shortcut = cfg.shortcut.clone();
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    gs.on_shortcut(shortcut.as_str(), |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            toggle_panel(app);
        }
    })
    .map_err(|e| format!("cannot bind shortcut {shortcut:?}: {e}"))?;
    *state.watcher.config.write().unwrap() = cfg;
    Ok(shortcut)
}

/// Delete clips past the configured retention. Returns how many were removed.
pub fn run_retention(app: &AppHandle) -> usize {
    let state = app.state::<AppState>();
    let mut store = state.store.lock().unwrap();
    let days = store.retention_days();
    let n = store.purge_older_than(days);
    drop(store);
    if n > 0 {
        eprintln!("klepp: retention removed {n} clip(s) older than {days} day(s)");
        let _ = app.emit("klepp://clips-changed", ());
    }
    n
}

fn main() {
    let store = Store::open().expect("cannot open ~/.klepp/klepp.db");
    eprintln!(
        "klepp: database at {}",
        store.root().join("klepp.db").display()
    );
    let (config, config_error) = match Config::load(store.root()) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(e)),
    };

    tauri::Builder::default()
        .runtime(tauri_runtime_wry::Wry::default())
        .manage(AppState {
            store: Mutex::new(store),
            watcher: Watcher::new(config),
            paused: AtomicBool::new(false),
        })
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![list, remove, pick, hide])
        // Serves image clips to the panel as klepp://localhost/image/<id>.
        .register_uri_scheme_protocol("klepp", |ctx, request| {
            let png = request
                .uri()
                .path()
                .strip_prefix("/image/")
                .and_then(|id| id.parse::<i64>().ok())
                .and_then(|id| {
                    ctx.app_handle()
                        .state::<AppState>()
                        .store
                        .lock()
                        .unwrap()
                        .image(id)
                });
            match png {
                Some(bytes) => Response::builder()
                    .header("Content-Type", "image/png")
                    .header("Cache-Control", "max-age=3600")
                    .body(bytes)
                    .unwrap(),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Vec::new())
                    .unwrap(),
            }
        })
        .setup(move |app| {
            // No Dock icon, no menu bar: Klepp lives behind its hotkey.
            app.set_activation_policy(ActivationPolicy::Accessory);

            let win = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Klepp")
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .visible_on_all_workspaces(true)
                .skip_taskbar(true)
                .resizable(false)
                .accept_first_mouse(true)
                .visible(false)
                .inner_size(900.0, PANEL_HEIGHT)
                .build()?;

            glass::apply(&win, PANEL_RADIUS);

            let handle = app.handle().clone();
            let shortcut = match apply_config(&handle) {
                Ok(s) => s,
                Err(e) => {
                    tray::alert(&handle, "Klepp could not apply config.toml", &e);
                    String::from("(none)")
                }
            };
            if let Some(e) = &config_error {
                tray::alert(&handle, "Klepp could not read config.toml", e);
            }
            tray::install(app, &shortcut)?;

            // `klepp --show` opens the panel right away (handy for testing).
            if std::env::args().any(|a| a == "--show") {
                show_panel(&handle);
            }

            let handle = app.handle().clone();
            win.on_window_event(move |event| {
                if let WindowEvent::Focused(false) = event {
                    let visible = handle
                        .get_webview_window("main")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    if visible {
                        hide_panel(&handle);
                    }
                }
            });

            let handle = app.handle().clone();
            let watcher = Arc::clone(&app.state::<AppState>().watcher);
            watcher.watch(move |clip| {
                let state = handle.state::<AppState>();
                if state.paused.load(Ordering::Relaxed) {
                    return;
                }
                let mut store = state.store.lock().unwrap();
                let added = match clip {
                    NewClip::Text { text, app } => store.add_text(text, app),
                    NewClip::Image {
                        png,
                        width,
                        height,
                        app,
                    } => store.add_image(png, width, height, app),
                };
                drop(store);
                if added.is_some() {
                    let _ = handle.emit("klepp://clips-changed", ());
                }
            });

            let handle = app.handle().clone();
            thread::Builder::new()
                .name("klepp-retention".into())
                .spawn(move || loop {
                    run_retention(&handle);
                    thread::sleep(RETENTION_INTERVAL);
                })
                .expect("spawn retention thread");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running klepp");
}
