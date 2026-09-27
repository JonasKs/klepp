//! "Launch at login" through SMAppService. Registration only works when
//! Klepp runs from a .app bundle; a bare `cargo run` binary reports NotFound.

use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::store::Store;

/// Settings key; "off" means the user disabled launch at login.
const KEY: &str = "login_item";

pub fn status() -> SMAppServiceStatus {
    // SAFETY: plain AppKit-style query on the main app's service.
    unsafe { SMAppService::mainAppService().status() }
}

pub fn is_enabled() -> bool {
    status() == SMAppServiceStatus::Enabled
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    // SAFETY: register/unregister are synchronous and need no extra state.
    let result = unsafe {
        let service = SMAppService::mainAppService();
        if on {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    result.map_err(|e| e.localizedDescription().to_string())
}

pub fn describe() -> &'static str {
    match status() {
        SMAppServiceStatus::Enabled => "enabled",
        SMAppServiceStatus::RequiresApproval => {
            "requires approval in System Settings → General → Login Items"
        }
        SMAppServiceStatus::NotRegistered => "not registered",
        _ => "not available (not running from a .app bundle?)",
    }
}

/// True when this binary runs from a bundle under /Applications.
pub fn is_installed() -> bool {
    std::env::current_exe()
        .map(|p| p.starts_with("/Applications"))
        .unwrap_or(false)
}

/// Register the installed app as a login item unless the user opted out.
pub fn register_if_installed(store: &Store) {
    if !is_installed() || store.setting(KEY).as_deref() == Some("off") || is_enabled() {
        return;
    }
    if let Err(e) = set_enabled(true) {
        eprintln!("klepp: could not register login item: {e}");
    }
}

/// Persist the user's choice and apply it to SMAppService.
pub fn apply_choice(store: &Store, on: bool) -> Result<(), String> {
    store.set_setting(KEY, if on { "on" } else { "off" });
    set_enabled(on)
}
