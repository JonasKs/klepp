//! Tiny append-only log at `~/.klepp/klepp.log`, for diagnosing things that
//! fail silently (paste, permissions, login item).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

const MAX_BYTES: u64 = 512 * 1024;

fn path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".klepp")
        .join("klepp.log")
}

/// Start a fresh file when the old one has grown large.
pub fn rotate() {
    let p = path();
    if fs::metadata(&p)
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false)
    {
        let _ = fs::rename(&p, p.with_extension("log.old"));
    }
}

pub fn line(message: impl AsRef<str>) {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let text = format!("{stamp} {}\n", message.as_ref());
    eprint!("klepp: {text}");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path()) {
        let _ = f.write_all(text.as_bytes());
    }
}
