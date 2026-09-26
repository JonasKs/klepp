//! Polls NSPasteboard and reports new text/image clips, skipping anything
//! that is flagged concealed/transient (1Password does this) or copied while
//! an ignored app is frontmost.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use objc2::runtime::ProtocolObject;
use objc2::AnyThread;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSPasteboard, NSPasteboardTypePNG,
    NSPasteboardTypeString, NSPasteboardTypeTIFF, NSPasteboardWriting, NSWorkspace,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};

use crate::config::Config;

const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Pasteboard types that mark a clip as "do not record" (see nspasteboard.org).
/// 1Password sets `ConcealedType` on everything it copies.
const IGNORED_PB_TYPES: &[&str] = &[
    "org.nspasteboard.ConcealedType",
    "org.nspasteboard.TransientType",
    "com.agilebits.onepassword",
];

pub enum NewClip {
    Text {
        text: String,
        app: Option<String>,
    },
    Image {
        png: Vec<u8>,
        width: u32,
        height: u32,
        app: Option<String>,
    },
}

/// Shared between the watcher and the rest of the app.
pub struct Watcher {
    pub config: RwLock<Config>,
    /// Pasteboard change count produced by our own writes; skipped by the poller.
    skip_change: AtomicI64,
}

impl Watcher {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Watcher {
            config: RwLock::new(config),
            skip_change: AtomicI64::new(-1),
        })
    }

    fn note_own_write(&self, pb: &NSPasteboard) {
        self.skip_change
            .store(pb.changeCount() as i64, Ordering::SeqCst);
    }

    /// Replace the clipboard contents with `text`.
    pub fn set_text(&self, text: &str) {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        pb.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
        self.note_own_write(&pb);
    }

    /// Replace the clipboard contents with an image (PNG bytes). Written via
    /// NSImage so every app gets the representation it prefers.
    pub fn set_image(&self, png: &[u8]) {
        let pb = NSPasteboard::generalPasteboard();
        let data = NSData::with_bytes(png);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            return;
        };
        let writable: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*image);
        pb.clearContents();
        pb.writeObjects(&NSArray::from_slice(&[writable]));
        self.note_own_write(&pb);
    }

    /// Runs forever on a background thread, calling `on_clip` for each new clip.
    pub fn watch(self: &Arc<Self>, on_clip: impl Fn(NewClip) + Send + 'static) {
        let me = Arc::clone(self);
        thread::Builder::new()
            .name("klepp-clipboard".into())
            .spawn(move || {
                let pb = NSPasteboard::generalPasteboard();
                let mut last_count = pb.changeCount();
                loop {
                    thread::sleep(POLL_INTERVAL);
                    let count = pb.changeCount();
                    if count == last_count {
                        continue;
                    }
                    last_count = count;
                    if count as i64 == me.skip_change.load(Ordering::SeqCst) {
                        continue;
                    }
                    if is_concealed(&pb) {
                        continue;
                    }
                    let (bundle_id, app) = frontmost_app();
                    let max_image = {
                        let cfg = me.config.read().unwrap();
                        if bundle_id
                            .as_deref()
                            .map(|b| cfg.ignores(b))
                            .unwrap_or(false)
                        {
                            continue;
                        }
                        cfg.max_image_bytes()
                    };
                    if let Some(text) = pb.stringForType(unsafe { NSPasteboardTypeString }) {
                        on_clip(NewClip::Text {
                            text: text.to_string(),
                            app,
                        });
                    } else if let Some((png, width, height)) = read_image(&pb, max_image) {
                        on_clip(NewClip::Image {
                            png,
                            width,
                            height,
                            app,
                        });
                    }
                }
            })
            .expect("spawn clipboard watcher");
    }
}

fn is_concealed(pb: &NSPasteboard) -> bool {
    let Some(types) = pb.types() else {
        return false;
    };
    types.iter().any(|t| {
        let t = t.to_string();
        IGNORED_PB_TYPES.iter().any(|ig| t.eq_ignore_ascii_case(ig))
    })
}

fn frontmost_app() -> (Option<String>, Option<String>) {
    let ws = NSWorkspace::sharedWorkspace();
    let Some(app) = ws.frontmostApplication() else {
        return (None, None);
    };
    let bundle = app.bundleIdentifier().map(|s| s.to_string());
    let name = app.localizedName().map(|s| s.to_string());
    (bundle, name)
}

/// PNG bytes plus pixel size of the image on the pasteboard, if any.
/// TIFF-only pasteboards (most apps) are converted to PNG.
fn read_image(pb: &NSPasteboard, max_bytes: usize) -> Option<(Vec<u8>, u32, u32)> {
    let png = if let Some(d) = pb.dataForType(unsafe { NSPasteboardTypePNG }) {
        d.to_vec()
    } else {
        let tiff = pb.dataForType(unsafe { NSPasteboardTypeTIFF })?;
        tiff_to_png(&tiff)?
    };
    if png.len() > max_bytes {
        return None;
    }
    let (w, h) = png_size(&png)?;
    Some((png, w, h))
}

fn tiff_to_png(data: &NSData) -> Option<Vec<u8>> {
    let rep = NSBitmapImageRep::imageRepWithData(data)?;
    let props = NSDictionary::new();
    // SAFETY: plain AppKit call with an empty, well-formed properties dictionary.
    let png =
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
    Some(png.to_vec())
}

/// Width and height from the PNG IHDR chunk.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((w, h))
}
