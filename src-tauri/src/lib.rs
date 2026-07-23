pub mod clipboard;
pub mod db;
pub mod paste_tracker;
pub mod security;

use db::{ClipItem, DatabaseManager};
use security::SecurityManager;
use std::sync::{mpsc, Arc, Mutex};
use tauri::{Emitter, Manager, State};

pub struct AppState {
    pub db: DatabaseManager,
    pub security: SecurityManager,
    pub active_clip_id: Mutex<Option<String>>,
}

#[tauri::command]
fn get_clips(state: State<'_, Arc<AppState>>, limit: Option<usize>) -> Result<Vec<ClipItem>, String> {
    state
        .db
        .get_recent_clips(limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn search_clips(state: State<'_, Arc<AppState>>, query: String) -> Result<Vec<ClipItem>, String> {
    if query.trim().is_empty() {
        return state.db.get_recent_clips(50).map_err(|e| e.to_string());
    }
    state.db.search_clips(&query).map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_to_clipboard(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    content: String,
) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::DataExchange::{EmptyClipboard, OpenClipboard, SetClipboardData, CloseClipboard};
        use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

        let target_text = if let Some(clip_id) = &id {
            if let Some(sensitive_str) = state.security.get_transient(clip_id) {
                sensitive_str
            } else {
                content
            }
        } else {
            content
        };

        let utf16: Vec<u16> = target_text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes_len = utf16.len() * 2;

        let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes_len);
        if hmem.is_err() {
            return Err("Failed to allocate global memory".to_string());
        }
        let hmem = hmem.unwrap();

        let ptr = GlobalLock(hmem);
        if ptr.is_null() {
            return Err("GlobalLock failed".to_string());
        }

        std::ptr::copy_nonoverlapping(utf16.as_ptr() as *const u8, ptr as *mut u8, bytes_len);
        let _ = GlobalUnlock(hmem);

        if OpenClipboard(HWND::default()).is_ok() {
            let _ = EmptyClipboard();
            const CF_UNICODETEXT: u32 = 13;
            let _ = SetClipboardData(CF_UNICODETEXT, windows::Win32::Foundation::HANDLE(hmem.0));
            let _ = CloseClipboard();

            if let Some(clip_id) = id {
                let _ = state.db.increment_paste(&clip_id);
            }
            return Ok(());
        }
    }

    Err("Could not copy to clipboard".to_string())
}

#[tauri::command]
fn delete_clip(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    state.security.clear_transient(&id);
    state.db.delete_clip(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_pin(state: State<'_, Arc<AppState>>, id: String) -> Result<bool, String> {
    state.db.toggle_pin(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_sensitive(state: State<'_, Arc<AppState>>, id: String) -> Result<String, String> {
    state
        .security
        .get_transient(&id)
        .ok_or_else(|| "Sensitive clip expired or not found".to_string())
}

#[tauri::command]
fn toggle_notch(window: tauri::Window) -> Result<(), String> {
    if let Ok(is_visible) = window.is_visible() {
        if is_visible {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
    Ok(())
}

pub fn run() {
    let db = DatabaseManager::new().expect("Failed to initialize SQLite database");
    let security = SecurityManager::new();

    let app_state = Arc::new(AppState {
        db,
        security,
        active_clip_id: Mutex::new(None),
    });

    let app_state_clip = Arc::clone(&app_state);

    tauri::Builder::default()
        .manage(Arc::clone(&app_state))
        .setup(move |app| {
            let handle = app.handle().clone();

            // 1. Start Win32 Clipboard Stream Listener
            let (clip_tx, clip_rx) = mpsc::channel::<clipboard::RawClipEvent>();
            clipboard::ClipboardListener::start_listening(clip_tx);

            // 2. Start Win32 Paste Tracker Hook
            let (paste_tx, paste_rx) = mpsc::channel::<paste_tracker::PasteEvent>();
            paste_tracker::PasteTracker::start_tracking(paste_tx);

            // 3. Start RAM TTL Security Cleanup Task
            app_state_clip.security.start_cleanup_task();

            let handle_clip = handle.clone();
            let state_clip = Arc::clone(&app_state_clip);

            // Processing thread for Captured Clips
            tokio::spawn(async move {
                while let Ok(raw_event) = clip_rx.recv() {
                    let is_sensitive = SecurityManager::is_sensitive_source(&raw_event.source_app)
                        || SecurityManager::is_sensitive_pattern(&raw_event.content);

                    let clip_id = format!("{:x}", md5_hash(&raw_event.content));
                    let category = classify_content(&raw_event.content, is_sensitive);

                    let display_content = if is_sensitive {
                        state_clip.security.store_transient(
                            clip_id.clone(),
                            raw_event.content.clone(),
                            raw_event.source_app.clone(),
                        );
                        "🔒 Password Protected".to_string()
                    } else {
                        raw_event.content.clone()
                    };

                    let item = ClipItem {
                        id: clip_id.clone(),
                        content: display_content,
                        source_app: raw_event.source_app.clone(),
                        category,
                        is_sensitive,
                        is_pinned: false,
                        created_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        paste_count: 0,
                    };

                    // Only save to DB if NOT sensitive or if masked representation
                    let _ = state_clip.db.insert_clip(&item);

                    // Emit real-time stream update to WebView Notch UI
                    let _ = handle_clip.emit("new-clip", &item);
                }
            });

            // Processing thread for Paste Tracking Events
            let handle_paste = handle.clone();
            tokio::spawn(async move {
                while let Ok(paste_event) = paste_rx.recv() {
                    let _ = handle_paste.emit("paste-event", &paste_event);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_clips,
            search_clips,
            copy_to_clipboard,
            delete_clip,
            toggle_pin,
            reveal_sensitive,
            toggle_notch
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn classify_content(content: &str, is_sensitive: bool) -> String {
    if is_sensitive {
        return "sensitive".to_string();
    }
    let trimmed = content.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("www.") {
        return "link".to_string();
    }
    if trimmed.contains('{') || trimmed.contains('}') || trimmed.contains("fn ") || trimmed.contains("const ") || trimmed.contains("function") {
        return "code".to_string();
    }
    "text".to_string()
}

fn md5_hash(input: &str) -> u128 {
    let mut hash: u128 = 0;
    for (i, byte) in input.bytes().enumerate() {
        hash = hash.wrapping_add((byte as u128).wrapping_shl((i % 16) as u32));
    }
    if hash == 0 {
        123456789
    } else {
        hash
    }
}
