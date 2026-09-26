//! Sends Cmd+V to the frontmost app. Needs Accessibility permission for the
//! running app; without it the event is silently ignored (the clip is still on
//! the clipboard, so the user can paste by hand).

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const KEY_V: CGKeyCode = 9;

pub fn send_cmd_v() {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    for down in [true, false] {
        if let Ok(ev) = CGEvent::new_keyboard_event(source.clone(), KEY_V, down) {
            ev.set_flags(CGEventFlags::CGEventFlagCommand);
            ev.post(CGEventTapLocation::HID);
        }
    }
}
