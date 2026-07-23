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
    pub category: String, // "text", "code", "link", "sensitive"
    pub is_sensitive: bool,
    pub is_pinned: bool,
    pub created_at: i64,
    pub paste_count: u32,
}

pub struct DatabaseManager {
    conn: Mutex<Connection>,
}

impl DatabaseManager {
    pub fn new() -> Result<Self> {
        let app_dir = dirs_data_dir().unwrap_or_else(|| PathBuf::from("./data"));
        fs::create_dir_all(&app_dir).ok();
        let db_path = app_dir.join("clipz.db");

        let conn = Connection::open(&db_path)?;

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
                paste_count INTEGER NOT NULL DEFAULT 0
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(
                id UNINDEXED,
                content,
                source_app,
                tokenize = 'porter unicode61'
            );

            CREATE TRIGGER IF NOT EXISTS clips_ai AFTER INSERT ON clips BEGIN
                INSERT INTO clips_fts(id, content, source_app) VALUES (new.id, new.content, new.source_app);
            END;

            CREATE TRIGGER IF NOT EXISTS clips_ad AFTER DELETE ON clips BEGIN
                INSERT INTO clips_fts(clips_fts, id, content, source_app) VALUES('delete', old.id, old.content, old.source_app);
            END;

            CREATE TRIGGER IF NOT EXISTS clips_au AFTER UPDATE ON clips BEGIN
                INSERT INTO clips_fts(clips_fts, id, content, source_app) VALUES('delete', old.id, old.content, old.source_app);
                INSERT INTO clips_fts(id, content, source_app) VALUES (new.id, new.content, new.source_app);
            END;
        ")?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_clip(&self, clip: &ClipItem) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO clips (id, content, source_app, category, is_sensitive, is_pinned, created_at, paste_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                clip.id,
                clip.content,
                clip.source_app,
                clip.category,
                if clip.is_sensitive { 1 } else { 0 },
                if clip.is_pinned { 1 } else { 0 },
                clip.created_at,
                clip.paste_count
            ],
        )?;
        Ok(())
    }

    pub fn get_recent_clips(&self, limit: usize) -> Result<Vec<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, content, source_app, category, is_sensitive, is_pinned, created_at, paste_count
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
        let formatted_query = format!("*{}*", query.replace('"', "\"\""));

        let mut stmt = conn.prepare(
            "SELECT c.id, c.content, c.source_app, c.category, c.is_sensitive, c.is_pinned, c.created_at, c.paste_count
             FROM clips c
             JOIN clips_fts fts ON c.id = fts.id
             WHERE clips_fts MATCH ?1
             ORDER BY c.is_pinned DESC, c.created_at DESC
             LIMIT 50",
        )?;

        let clip_iter = stmt.query_map(params![formatted_query], |row| {
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
        conn.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
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

    pub fn increment_paste(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE clips SET paste_count = paste_count + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }
}

fn dirs_data_dir() -> Option<PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        Some(PathBuf::from(appdata).join("clipz"))
    } else {
        None
    }
}
