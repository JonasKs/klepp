//! Menu bar status item: open the panel, pause recording, auto-delete
//! schedule, config access, quit.

use std::sync::atomic::Ordering;

use objc2::MainThreadMarker;
use objc2_app_kit::NSAlert;
use objc2_foundation::NSString;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager};

use crate::AppState;

const ID_OPEN: &str = "open";
const ID_PAUSE: &str = "pause";
const ID_LOGIN: &str = "login";
const ID_FOLDER: &str = "folder";
const ID_EDIT_CONFIG: &str = "edit-config";
const ID_RELOAD_CONFIG: &str = "reload-config";
const ID_QUIT: &str = "quit";
const RETENTION_PREFIX: &str = "retention-";

/// (days, label); 0 keeps everything.
const RETENTION_CHOICES: &[(u32, &str)] = &[
    (0, "Never"),
    (1, "1 day"),
    (7, "7 days"),
    (30, "30 days"),
    (90, "90 days"),
    (365, "1 year"),
];

fn open_label(shortcut: &str) -> String {
    format!("Open Klepp\t{shortcut}")
}

pub fn install(app: &App, shortcut: &str) -> tauri::Result<()> {
    let current_days = app
        .state::<AppState>()
        .store
        .lock()
        .unwrap()
        .retention_days();

    let open = MenuItem::with_id(app, ID_OPEN, open_label(shortcut), true, None::<&str>)?;
    let pause =
        CheckMenuItem::with_id(app, ID_PAUSE, "Pause recording", true, false, None::<&str>)?;
    let login = CheckMenuItem::with_id(
        app,
        ID_LOGIN,
        "Launch at login",
        true,
        crate::login::is_enabled(),
        None::<&str>,
    )?;

    let retention_items: Vec<CheckMenuItem<_>> = RETENTION_CHOICES
        .iter()
        .map(|(days, label)| {
            CheckMenuItem::with_id(
                app,
                format!("{RETENTION_PREFIX}{days}"),
                *label,
                true,
                *days == current_days,
                None::<&str>,
            )
        })
        .collect::<Result<_, _>>()?;
    let retention = Submenu::with_id(app, "retention", "Delete clips older than", true)?;
    for item in &retention_items {
        retention.append(item)?;
    }

    let folder = MenuItem::with_id(
        app,
        ID_FOLDER,
        "Show database in Finder",
        true,
        None::<&str>,
    )?;
    let edit_cfg = MenuItem::with_id(app, ID_EDIT_CONFIG, "Edit config.toml…", true, None::<&str>)?;
    let reload_cfg = MenuItem::with_id(app, ID_RELOAD_CONFIG, "Reload config", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "Quit Klepp", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &login,
            &retention,
            &PredefinedMenuItem::separator(app)?,
            &folder,
            &edit_cfg,
            &reload_cfg,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Klepp")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            match id {
                ID_OPEN => crate::show_panel(app),
                ID_PAUSE => {
                    let paused = pause.is_checked().unwrap_or(false);
                    app.state::<AppState>()
                        .paused
                        .store(paused, Ordering::Relaxed);
                }
                ID_LOGIN => {
                    let on = login.is_checked().unwrap_or(false);
                    let state = app.state::<AppState>();
                    let store = state.store.lock().unwrap();
                    if let Err(e) = crate::login::apply_choice(&store, on) {
                        let _ = login.set_checked(!on);
                        alert(app, "Klepp could not change the login item", &e);
                    }
                }
                ID_FOLDER => {
                    let root = app
                        .state::<AppState>()
                        .store
                        .lock()
                        .unwrap()
                        .root()
                        .to_path_buf();
                    let _ = std::process::Command::new("open").arg(root).spawn();
                }
                ID_EDIT_CONFIG => {
                    let root = app
                        .state::<AppState>()
                        .store
                        .lock()
                        .unwrap()
                        .root()
                        .to_path_buf();
                    let path = crate::config::Config::path(&root);
                    let _ = std::process::Command::new("open")
                        .arg("-t")
                        .arg(path)
                        .spawn();
                }
                ID_RELOAD_CONFIG => match crate::apply_config(app) {
                    Ok(shortcut) => {
                        let _ = open.set_text(open_label(&shortcut));
                    }
                    Err(e) => alert(app, "Klepp could not apply config.toml", &e),
                },
                ID_QUIT => app.exit(0),
                other => {
                    if let Some(days) = other
                        .strip_prefix(RETENTION_PREFIX)
                        .and_then(|d| d.parse::<u32>().ok())
                    {
                        for item in &retention_items {
                            let mine = item.id().as_ref() == other;
                            let _ = item.set_checked(mine);
                        }
                        app.state::<AppState>()
                            .store
                            .lock()
                            .unwrap()
                            .set_retention_days(days);
                        crate::run_retention(app);
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// Native alert, shown from the main thread.
pub fn alert(app: &AppHandle, title: &str, message: &str) {
    let (title, message) = (title.to_string(), message.to_string());
    let _ = app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("main thread");
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(&title));
        alert.setInformativeText(&NSString::from_str(&message));
        let _ = alert.runModal();
    });
}
