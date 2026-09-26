//! SQLite-backed clip store at `~/.klepp/klepp.db`. Text clips and PNG image
//! blobs live in one `clips` table; GUI-set preferences in `settings`.

use chrono::{DateTime, Local, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Text clips larger than this are not recorded (keeps the store snappy).
const MAX_TEXT_BYTES: usize = 1024 * 1024;
/// Max number of results returned to the UI per query.
const MAX_RESULTS: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Image => "image",
        }
    }
    fn parse(s: &str) -> Kind {
        if s == "image" {
            Kind::Image
        } else {
            Kind::Text
        }
    }
}

/// A clip row without the image blob (fetch that with [`Store::image`]).
#[derive(Clone, Debug)]
pub struct Clip {
    pub id: i64,
    pub ts: DateTime<Local>,
    pub kind: Kind,
    pub text: String,
    pub width: u32,
    pub height: u32,
    pub app: Option<String>,
}

pub struct Store {
    path: PathBuf,
    db: Connection,
}

fn content_hash(bytes: &[u8]) -> i64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish() as i64
}

impl Store {
    pub fn open() -> rusqlite::Result<Self> {
        let root = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".klepp");
        let _ = std::fs::create_dir_all(&root);
        Self::open_at(root.join("klepp.db"))
    }

    pub fn open_at(path: PathBuf) -> rusqlite::Result<Self> {
        let db = Connection::open(&path)?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS clips (
               id      INTEGER PRIMARY KEY AUTOINCREMENT,
               ts      INTEGER NOT NULL,
               kind    TEXT    NOT NULL,
               text    TEXT    NOT NULL DEFAULT '',
               search  TEXT    NOT NULL DEFAULT '',
               image   BLOB,
               width   INTEGER NOT NULL DEFAULT 0,
               height  INTEGER NOT NULL DEFAULT 0,
               hash    INTEGER NOT NULL,
               app     TEXT
             );
             CREATE INDEX IF NOT EXISTS clips_ts   ON clips(ts DESC);
             CREATE INDEX IF NOT EXISTS clips_hash ON clips(hash);
             CREATE TABLE IF NOT EXISTS settings (
               key   TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );",
        )?;
        Ok(Store { path, db })
    }

    /// Directory holding the database.
    pub fn root(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    /// Record a text clip. Returns `None` if it was skipped (empty, too big,
    /// or identical to the most recent clip). An older identical clip is
    /// removed so the history stays de-duplicated and the clip moves to the top.
    pub fn add_text(&mut self, text: String, app: Option<String>) -> Option<Clip> {
        if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
            return None;
        }
        let hash = content_hash(text.as_bytes());
        let search = text.to_lowercase();
        self.insert(Kind::Text, hash, &search, &text, None, 0, 0, app)
    }

    /// Record an image clip (PNG bytes). De-duplicated by content hash.
    pub fn add_image(
        &mut self,
        png: Vec<u8>,
        width: u32,
        height: u32,
        app: Option<String>,
    ) -> Option<Clip> {
        if png.is_empty() {
            return None;
        }
        let hash = content_hash(&png);
        let search = format!(
            "image png {width}x{height} {}",
            app.as_deref().unwrap_or("")
        )
        .to_lowercase();
        self.insert(
            Kind::Image,
            hash,
            &search,
            "",
            Some(&png),
            width,
            height,
            app,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self,
        kind: Kind,
        hash: i64,
        search: &str,
        text: &str,
        image: Option<&[u8]>,
        width: u32,
        height: u32,
        app: Option<String>,
    ) -> Option<Clip> {
        // Same as the newest clip: nothing to do.
        let newest: Option<(i64, String)> = self
            .db
            .query_row(
                "SELECT hash, kind FROM clips ORDER BY ts DESC, id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten();
        if newest == Some((hash, kind.as_str().to_string())) {
            return None;
        }
        // Drop older duplicates (move-to-top semantics).
        let _ = self.db.execute(
            "DELETE FROM clips WHERE hash = ?1 AND kind = ?2",
            params![hash, kind.as_str()],
        );

        let ts = Local::now();
        self.db
            .execute(
                "INSERT INTO clips (ts, kind, text, search, image, width, height, hash, app)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    ts.timestamp_millis(),
                    kind.as_str(),
                    text,
                    search,
                    image,
                    width,
                    height,
                    hash,
                    app
                ],
            )
            .ok()?;
        Some(Clip {
            id: self.db.last_insert_rowid(),
            ts,
            kind,
            text: text.to_string(),
            width,
            height,
            app,
        })
    }

    pub fn get(&self, id: i64) -> Option<Clip> {
        self.db
            .query_row(
                "SELECT id, ts, kind, text, width, height, app FROM clips WHERE id = ?1",
                [id],
                row_to_clip,
            )
            .optional()
            .ok()
            .flatten()
    }

    /// PNG bytes of an image clip.
    pub fn image(&self, id: i64) -> Option<Vec<u8>> {
        self.db
            .query_row("SELECT image FROM clips WHERE id = ?1", [id], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn delete(&mut self, id: i64) -> bool {
        self.db
            .execute("DELETE FROM clips WHERE id = ?1", [id])
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    /// Delete clips older than `days` days. Returns how many were removed.
    pub fn purge_older_than(&mut self, days: u32) -> usize {
        if days == 0 {
            return 0;
        }
        let cutoff = Local::now().timestamp_millis() - i64::from(days) * 86_400_000;
        let n = self
            .db
            .execute("DELETE FROM clips WHERE ts < ?1", [cutoff])
            .unwrap_or(0);
        if n > 0 {
            let _ = self.db.execute_batch("VACUUM");
        }
        n
    }

    /// Case-insensitive search: every whitespace-separated term must occur.
    /// Newest first.
    pub fn search(&self, query: &str) -> Vec<Clip> {
        let terms: Vec<String> = query.split_whitespace().map(|t| t.to_lowercase()).collect();
        let mut sql = String::from("SELECT id, ts, kind, text, width, height, app FROM clips");
        let mut args: Vec<String> = Vec::new();
        for (i, t) in terms.iter().enumerate() {
            sql.push_str(if i == 0 { " WHERE " } else { " AND " });
            sql.push_str("instr(search, ?) > 0");
            args.push(t.clone());
        }
        sql.push_str(&format!(" ORDER BY ts DESC, id DESC LIMIT {MAX_RESULTS}"));
        let Ok(mut stmt) = self.db.prepare(&sql) else {
            return Vec::new();
        };
        stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_clip)
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    // ----- settings -----

    pub fn setting(&self, key: &str) -> Option<String> {
        self.db
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&mut self, key: &str, value: &str) {
        let _ = self.db.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        );
    }

    /// Auto-delete horizon in days; 0 means keep forever.
    pub fn retention_days(&self) -> u32 {
        self.setting("retention_days")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    pub fn set_retention_days(&mut self, days: u32) {
        self.set_setting("retention_days", &days.to_string());
    }
}

fn row_to_clip(r: &rusqlite::Row) -> rusqlite::Result<Clip> {
    let millis: i64 = r.get(1)?;
    let kind: String = r.get(2)?;
    Ok(Clip {
        id: r.get(0)?,
        ts: Local
            .timestamp_millis_opt(millis)
            .single()
            .unwrap_or_else(Local::now),
        kind: Kind::parse(&kind),
        text: r.get(3)?,
        width: r.get::<_, i64>(4)? as u32,
        height: r.get::<_, i64>(5)? as u32,
        app: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("klepp-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("klepp.db")
    }

    #[test]
    fn text_add_search_delete_roundtrip() {
        let db = tmp("text");
        let mut s = Store::open_at(db.clone()).unwrap();
        assert!(
            s.add_text("   ".into(), None).is_none(),
            "blank clips are skipped"
        );
        let a = s
            .add_text("hello world".into(), Some("Test".into()))
            .unwrap();
        let _b = s.add_text("Hello Rust".into(), None).unwrap();
        assert!(
            s.add_text("Hello Rust".into(), None).is_none(),
            "same as newest is skipped"
        );

        assert_eq!(s.search("").len(), 2);
        assert_eq!(s.search("HELLO").len(), 2);
        assert_eq!(s.search("hello world").len(), 1);
        assert_eq!(s.search("hello rust").len(), 1);
        assert_eq!(s.search("nope").len(), 0);

        // Re-adding an older clip moves it to the top without duplicating it.
        let a2 = s.add_text("hello world".into(), None).unwrap();
        assert_ne!(a.id, a2.id);
        assert_eq!(s.search("").len(), 2);
        assert_eq!(s.search("")[0].text, "hello world");
        assert!(s.get(a.id).is_none());

        // Persisted and reloadable.
        drop(s);
        let mut s = Store::open_at(db).unwrap();
        assert_eq!(s.search("").len(), 2);
        assert_eq!(s.search("")[0].text, "hello world");

        assert!(s.delete(a2.id));
        assert!(!s.delete(a2.id));
        assert_eq!(s.search("").len(), 1);
    }

    #[test]
    fn image_add_dedupe_delete() {
        let mut s = Store::open_at(tmp("image")).unwrap();
        let png = vec![1u8, 2, 3, 4];
        let a = s
            .add_image(png.clone(), 10, 20, Some("Preview".into()))
            .unwrap();
        assert_eq!(s.image(a.id).unwrap(), png);
        assert!(
            s.add_image(png.clone(), 10, 20, None).is_none(),
            "same image as newest is skipped"
        );
        s.add_text("in between".into(), None).unwrap();
        let a2 = s.add_image(png, 10, 20, None).unwrap();
        assert_ne!(a.id, a2.id);
        assert!(s.get(a.id).is_none(), "old duplicate image removed");
        assert_eq!(s.search("").len(), 2);
        assert_eq!(s.search("image 10x20").len(), 1);
        assert_eq!(s.search("between").len(), 1);
        let top = &s.search("")[0];
        assert_eq!(top.kind, Kind::Image);
        assert_eq!((top.width, top.height), (10, 20));
        assert!(s.delete(a2.id));
        assert!(s.image(a2.id).is_none());
    }

    #[test]
    fn retention_purges_old_clips() {
        let mut s = Store::open_at(tmp("purge")).unwrap();
        let old = s.add_text("old".into(), None).unwrap();
        let fresh = s.add_text("fresh".into(), None).unwrap();
        let eight_days_ago = Local::now().timestamp_millis() - 8 * 86_400_000;
        s.db.execute(
            "UPDATE clips SET ts = ?1 WHERE id = ?2",
            params![eight_days_ago, old.id],
        )
        .unwrap();
        assert_eq!(s.purge_older_than(0), 0, "0 days means keep forever");
        assert_eq!(s.purge_older_than(7), 1);
        assert!(s.get(old.id).is_none());
        assert!(s.get(fresh.id).is_some());

        assert_eq!(s.retention_days(), 0);
        s.set_retention_days(30);
        assert_eq!(s.retention_days(), 30);
    }
}
