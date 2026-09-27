//! Liquid Glass background (NSGlassEffectView, macOS 26+) inserted underneath
//! the transparent webview, plus a few panel-style tweaks on the NSWindow.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowOrderingMode,
};

pub fn apply(window: &tauri::WebviewWindow, radius: f64) {
    let ptr = window.ns_window().expect("ns_window") as usize;
    let _ = window.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("must run on main thread");
        // SAFETY: tauri hands us a valid NSWindow pointer that lives as long as the window.
        let ns_window: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
        install(mtm, ns_window, radius);
    });
}

fn install(mtm: MainThreadMarker, win: &NSWindow, radius: f64) {
    let Some(content) = win.contentView() else {
        return;
    };
    let frame = content.bounds();

    let glass = NSGlassEffectView::initWithFrame(mtm.alloc(), frame);
    glass.setCornerRadius(radius);
    glass.setStyle(NSGlassEffectViewStyle::Regular);
    let glass: Retained<NSView> = Retained::into_super(glass);
    glass.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, None);

    win.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    // A transparent window's shadow is computed as a rectangle here, which
    // shows up as a square dark corner outside the rounded glass.
    win.setHasShadow(false);
}
