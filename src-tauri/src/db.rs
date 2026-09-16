use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipItem {
    pub id: String,
    pub content: String,
    pub source_app: String,
    pub category: String, // "text", "code", "link", "sensitive", "image"
    pub is_sensitive: bool,
    pub is_pinned: bool,
    pub created_at: i64,
    pub paste_count: u32,
    pub reminder_at: Option<i64>,
    pub ocr_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasteLogItem {
    pub id: i64,
    pub clip_id: String,
    pub target_app: String,
    pub pasted_at: i64,
}

pub struct DatabaseManager {
    conn: Mutex<Connection>,
}

impl DatabaseManager {
    /// Open the clip database in this OS's per-user data folder.
    ///
    /// The path is always ABSOLUTE. This used to fall back to a relative
    /// "./data", which resolves against the folder the process happens to be
    /// started from. A desktop launch starts at "/" on macOS and at
    /// C:\Windows\System32 on Windows, so the database could not be created
    /// there and the app aborted before it ever drew a window. It only ever
    /// worked when launched from a terminal that happened to sit in a folder
    /// with a writable "data" directory.
    pub fn new() -> std::result::Result<Self, String> {
        let app_dir = app_data_dir().ok_or_else(|| {
            "Could not work out where to keep Clipz data: neither APPDATA nor HOME is set.".to_string()
        })?;
        fs::create_dir_all(&app_dir).map_err(|e| {
            format!("Could not create the Clipz data folder at {}: {e}", app_dir.display())
        })?;
        let db_path = app_dir.join("clipz.db");

        Self::open_at(&db_path)
            .map_err(|e| format!("Could not open the clip database at {}: {e}", db_path.display()))
    }

    fn open_at(db_path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        // Enable WAL mode & foreign keys
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
        ")?;

        // Initialize schema
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS clips (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source_app TEXT NOT NULL,
                category TEXT NOT NULL,
                is_sensitive INTEGER NOT NULL DEFAULT 0,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                paste_count INTEGER NOT NULL DEFAULT 0,
                reminder_at INTEGER,
                ocr_text TEXT
            );

            CREATE TABLE IF NOT EXISTS paste_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_id TEXT NOT NULL,
                target_app TEXT NOT NULL,
                pasted_at INTEGER NOT NULL,
                FOREIGN KEY(clip_id) REFERENCES clips(id) ON DELETE CASCADE
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(
                id UNINDEXED,
                content,
                source_app,
                ocr_text,
                tokenize = 'porter unicode61'
            );

            -- Re-create triggers ensuring Base64 image content is NOT tokenized in FTS
            DROP TRIGGER IF EXISTS clips_ai;
            DROP TRIGGER IF EXISTS clips_ad;
            DROP TRIGGER IF EXISTS clips_au;

            CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN
                INSERT INTO clips_fts(id, content, source_app, ocr_text) 
                VALUES (
                    new.id, 
                    CASE WHEN new.category = 'image' THEN '' ELSE new.content END, 
                    new.source_app, 
                    COALESCE(new.ocr_text, '')
                );
            END;

            -- `clips_fts` is an ordinary FTS5 table, so its rows are removed with a
            -- plain DELETE. The FTS5 'delete' command is only valid when the caller
            -- can supply the matching rowid, which the triggers do not have.
            CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN
                DELETE FROM clips_fts WHERE id = old.id;
            END;

            CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN
                DELETE FROM clips_fts WHERE id = old.id;
                INSERT INTO clips_fts(id, content, source_app, ocr_text) 
                VALUES (
                    new.id, 
                    CASE WHEN new.category = 'image' THEN '' ELSE new.content END, 
                    new.source_app, 
                    COALESCE(new.ocr_text, '')
                );
            END;

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
        ")?;

        // Migration for existing databases: ensure reminder_at and ocr_text columns exist
        let _ = conn.execute("ALTER TABLE clips ADD COLUMN reminder_at INTEGER;", []);
        let _ = conn.execute("ALTER TABLE clips ADD COLUMN ocr_text TEXT;", []);
        // Sealed bytes for sensitive clips. Deliberately a separate column:
        // `content` keeps the mask the UI shows and the FTS triggers index, so
        // secrets never reach the search index.
        let _ = conn.execute("ALTER TABLE clips ADD COLUMN secret_blob BLOB;", []);

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_clip(&self, clip: &ClipItem) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO clips (id, content, source_app, category, is_sensitive, is_pinned, created_at, paste_count, reminder_at, ocr_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                clip.id,
                clip.content,
                clip.source_app,
                clip.category,
                if clip.is_sensitive { 1 } else { 0 },
                if clip.is_pinned { 1 } else { 0 },
                clip.created_at,
                clip.paste_count,
                clip.reminder_at,
                clip.ocr_text
            ],
        )?;
        Ok(())
    }

    /// Store the sealed bytes of a sensitive clip.
    pub fn set_secret_blob(&self, id: &str, blob: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("UPDATE clips SET secret_blob = ?1 WHERE id = ?2", params![blob, id])?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    /// The sealed bytes of a sensitive clip, if it has any.
    pub fn get_secret_blob(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT secret_blob FROM clips WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => Ok(row.get::<_, Option<Vec<u8>>>(0)?),
            None => Ok(None),
        }
    }

    pub fn update_ocr_text(&self, id: &str, ocr_text: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE clips SET ocr_text = ?1 WHERE id = ?2",
            params![ocr_text, id],
        )?;
        Ok(())
    }

    pub fn get_recent_clips(&self, limit: usize) -> Result<Vec<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, content, source_app, category, is_sensitive, is_pinned, created_at, paste_count, reminder_at, ocr_text
             FROM clips ORDER BY is_pinned DESC, created_at DESC LIMIT ?1",
        )?;

        let clip_iter = stmt.query_map(params![limit as i64], |row| {
            let sensitive_int: i32 = row.get(4)?;
            let pinned_int: i32 = row.get(5)?;
            Ok(ClipItem {
                id: row.get(0)?,
                content: row.get(1)?,
                source_app: row.get(2)?,
                category: row.get(3)?,
                is_sensitive: sensitive_int != 0,
                is_pinned: pinned_int != 0,
                created_at: row.get(6)?,
                paste_count: row.get(7)?,
                reminder_at: row.get(8)?,
                ocr_text: row.get(9)?,
            })
        })?;

        let mut items = Vec::new();
        for item in clip_iter {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn search_clips(&self, query: &str) -> Result<Vec<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        let clean_query = query.trim();
        if clean_query.is_empty() {
            drop(conn);
            return self.get_recent_clips(50);
        }

        // 1. Try SQLite FTS5 Full-Text Match
        let sanitized_fts = clean_query.replace('"', "");
        let fts_query = format!("\"{}\"*", sanitized_fts);

        let fts_res = conn.prepare(
            "SELECT c.id, c.content, c.source_app, c.category, c.is_sensitive, c.is_pinned, c.created_at, c.paste_count, c.reminder_at, c.ocr_text
             FROM clips c
             JOIN clips_fts fts ON c.id = fts.id
             WHERE clips_fts MATCH ?1
             ORDER BY c.is_pinned DESC, c.created_at DESC
             LIMIT 50",
        );

        if let Ok(mut stmt) = fts_res {
            let clip_iter = stmt.query_map(params![fts_query], |row| {
                let sensitive_int: i32 = row.get(4)?;
                let pinned_int: i32 = row.get(5)?;
                Ok(ClipItem {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source_app: row.get(2)?,
                    category: row.get(3)?,
                    is_sensitive: sensitive_int != 0,
                    is_pinned: pinned_int != 0,
                    created_at: row.get(6)?,
                    paste_count: row.get(7)?,
                    reminder_at: row.get(8)?,
                    ocr_text: row.get(9)?,
                })
            });

            if let Ok(iter) = clip_iter {
                let mut items = Vec::new();
                for item in iter {
                    if let Ok(clip) = item {
                        items.push(clip);
                    }
                }
                if !items.is_empty() {
                    return Ok(items);
                }
            }
        }

        // 2. Fallback Substring Search (handles URLs, symbols, code snippets, OCR text, etc.)
        let like_param = format!("%{}%", clean_query);
        let mut fallback_stmt = conn.prepare(
            "SELECT id, content, source_app, category, is_sensitive, is_pinned, created_at, paste_count, reminder_at, ocr_text
             FROM clips
             WHERE content LIKE ?1 OR source_app LIKE ?1 OR ocr_text LIKE ?1
             ORDER BY is_pinned DESC, created_at DESC
             LIMIT 50",
        )?;

        let clip_iter = fallback_stmt.query_map(params![like_param], |row| {
            let sensitive_int: i32 = row.get(4)?;
            let pinned_int: i32 = row.get(5)?;
            Ok(ClipItem {
                id: row.get(0)?,
                content: row.get(1)?,
                source_app: row.get(2)?,
                category: row.get(3)?,
                is_sensitive: sensitive_int != 0,
                is_pinned: pinned_int != 0,
                created_at: row.get(6)?,
                paste_count: row.get(7)?,
                reminder_at: row.get(8)?,
                ocr_text: row.get(9)?,
            })
        })?;

        let mut items = Vec::new();
        for item in clip_iter {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn delete_clip(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM paste_logs WHERE clip_id = ?1", params![id]);
        let _ = conn.execute("DELETE FROM clips WHERE id = ?1", params![id]);
        Ok(())
    }

    pub fn toggle_pin(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let current: i32 = conn.query_row(
            "SELECT is_pinned FROM clips WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        let new_state = if current == 0 { 1 } else { 0 };
        conn.execute(
            "UPDATE clips SET is_pinned = ?1 WHERE id = ?2",
            params![new_state, id],
        )?;
        Ok(new_state != 0)
    }

    pub fn set_clip_reminder(&self, id: &str, reminder_at: Option<i64>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("ALTER TABLE clips ADD COLUMN reminder_at INTEGER;", []);
        conn.execute(
            "UPDATE clips SET reminder_at = ?1 WHERE id = ?2",
            params![reminder_at, id],
        )?;
        Ok(())
    }

    pub fn increment_paste(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE clips SET paste_count = paste_count + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn log_paste(&self, clip_id: &str, target_app: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO paste_logs (clip_id, target_app, pasted_at) VALUES (?1, ?2, ?3)",
            params![clip_id, target_app, now],
        )?;

        conn.execute(
            "UPDATE clips SET paste_count = paste_count + 1 WHERE id = ?1",
            params![clip_id],
        )?;

        Ok(())
    }

    pub fn get_paste_history(&self, clip_id: &str) -> Result<Vec<PasteLogItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, clip_id, target_app, pasted_at FROM paste_logs WHERE clip_id = ?1 ORDER BY pasted_at DESC LIMIT 50",
        )?;

        let iter = stmt.query_map(params![clip_id], |row| {
            Ok(PasteLogItem {
                id: row.get(0)?,
                clip_id: row.get(1)?,
                target_app: row.get(2)?,
                pasted_at: row.get(3)?,
            })
        })?;

        let mut items = Vec::new();
        for item in iter {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let val: String = row.get(0)?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn clear_all_clips(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM paste_logs", []);
        let _ = conn.execute("DELETE FROM clips", []);
        let _ = conn.execute("DELETE FROM settings", []);
        Ok(())
    }
}

/// The per-user folder Clipz keeps its database in, absolute on every OS.
///
/// Windows keeps its historic %APPDATA%\clipz location, so existing installs
/// keep their clips. macOS and Linux had no branch here at all, which is what
/// produced the relative "./data" fallback and the startup crash.
pub fn app_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("clipz"))
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support/Clipz"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .map(|base| base.join("clipz"))
    }
}
