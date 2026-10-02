//! Pastes into the app that was in front before the panel opened: re-activate
//! it, wait until it really is frontmost, then send Cmd+V.
//!
//! Sending keystrokes needs Accessibility permission. A grant made for an
//! ad-hoc signed build is bound to that binary's hash and does not carry over
//! to later builds; `tccutil reset Accessibility com.jonas.klepp` clears it.

use std::thread;
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::CFString;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

use crate::log;

const KEY_V: CGKeyCode = 9;
const ACTIVATE_TIMEOUT: Duration = Duration::from_millis(800);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

pub fn is_trusted() -> bool {
    // SAFETY: plain query with no arguments.
    unsafe { AXIsProcessTrusted() }
}

/// Ask macOS to show its "allow Klepp to control this computer" dialog.
pub fn prompt_for_trust() {
    let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
    let options =
        CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
    // SAFETY: the dictionary outlives the call.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
}

/// The app a pick pastes into.
#[derive(Clone, Debug)]
pub struct Target {
    pub pid: i32,
    pub name: String,
    pub bundle_id: Option<String>,
}

/// Which paste shortcut to send. Terminals paste text on Cmd+V, but programs
/// running inside them (Claude Code) take images on Ctrl+V.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    CmdV,
    CtrlV,
}

/// The frontmost app, unless that is Klepp itself.
pub fn frontmost_other_app() -> Option<Target> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    if pid == std::process::id() as i32 {
        return None;
    }
    Some(Target {
        pid,
        name: app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        bundle_id: app.bundleIdentifier().map(|s| s.to_string()),
    })
}

fn frontmost_pid() -> Option<i32> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|a| a.processIdentifier())
}

/// Bring `pid` to the front and wait until macOS reports it frontmost.
fn activate(pid: i32) -> bool {
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return false;
    };
    app.activateWithOptions(NSApplicationActivationOptions(0));
    let start = Instant::now();
    while start.elapsed() < ACTIVATE_TIMEOUT {
        if frontmost_pid() == Some(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(15));
    }
    frontmost_pid() == Some(pid)
}

fn send(key: Key) -> bool {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return false;
    };
    let flags = match key {
        Key::CmdV => CGEventFlags::CGEventFlagCommand,
        Key::CtrlV => CGEventFlags::CGEventFlagControl,
    };
    let mut ok = true;
    for down in [true, false] {
        match CGEvent::new_keyboard_event(source.clone(), KEY_V, down) {
            Ok(ev) => {
                ev.set_flags(flags);
                ev.post(CGEventTapLocation::AnnotatedSession);
            }
            Err(_) => ok = false,
        }
        thread::sleep(Duration::from_millis(12));
    }
    ok
}

/// Paste the current clipboard into `target` (the app that was in front when
/// the panel opened). Everything is logged so silent failures are traceable.
pub fn paste_into(target: Option<Target>, key: Key) {
    if !is_trusted() {
        log::line(
            "paste: skipped, Klepp lacks Accessibility permission (clip is on the clipboard)",
        );
        prompt_for_trust();
        return;
    }
    match target {
        Some(t) => {
            let front = activate(t.pid);
            log::line(format!(
                "paste: target {:?} (pid {}) frontmost={front}, actual frontmost pid={:?}",
                t.name,
                t.pid,
                frontmost_pid()
            ));
        }
        None => {
            // No known target: give macOS a moment to restore the previous app.
            thread::sleep(Duration::from_millis(150));
            log::line(format!(
                "paste: no recorded target, frontmost pid={:?}",
                frontmost_pid()
            ));
        }
    }
    thread::sleep(Duration::from_millis(40));
    let sent = send(key);
    log::line(format!("paste: {key:?} sent={sent}"));
}
