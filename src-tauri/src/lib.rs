pub mod clipboard;
pub mod db;
pub mod paste_tracker;
pub mod security;

use db::{ClipItem, DatabaseManager};
use security::SecurityManager;
use std::sync::{mpsc, Arc, Mutex};
use tauri::{Emitter, Manager as _, State};

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
fn get_paste_history(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<db::PasteLogItem>, String> {
    state.db.get_paste_history(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_to_clipboard(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    content: String,
    auto_paste: Option<bool>,
) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::DataExchange::{EmptyClipboard, OpenClipboard, SetClipboardData, CloseClipboard};
        use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
        use clipboard::base64_decode;

        let target_text = if let Some(clip_id) = &id {
            if let Some(sensitive_str) = state.security.get_transient(clip_id) {
                sensitive_str
            } else {
                content
            }
        } else {
            content
        };

        if let Some(clip_id) = &id {
            *state.active_clip_id.lock().unwrap() = Some(clip_id.clone());
        }

        let target_app = paste_tracker::get_active_app_name();

        if target_text.starts_with("data:image/") {
            if let Some(pos) = target_text.find(";base64,") {
                let b64_str = &target_text[pos + 8..];
                if let Ok(bmp_bytes) = base64_decode(b64_str) {
                    let dib_bytes = if bmp_bytes.len() > 14 && &bmp_bytes[0..2] == b"BM" {
                        &bmp_bytes[14..]
                    } else {
                        &bmp_bytes[..]
                    };

                    let hmem = GlobalAlloc(GMEM_MOVEABLE, dib_bytes.len());
                    if let Ok(hmem) = hmem {
                        let ptr = GlobalLock(hmem);
                        if !ptr.is_null() {
                            std::ptr::copy_nonoverlapping(dib_bytes.as_ptr(), ptr as *mut u8, dib_bytes.len());
                            let _ = GlobalUnlock(hmem);

                            if OpenClipboard(HWND::default()).is_ok() {
                                let _ = EmptyClipboard();
                                const CF_DIB: u32 = 8;
                                let _ = SetClipboardData(CF_DIB, windows::Win32::Foundation::HANDLE(hmem.0));
                                let _ = CloseClipboard();

                                if let Some(clip_id) = &id {
                                    let _ = state.db.log_paste(clip_id, &target_app);
                                }

                                if auto_paste.unwrap_or(false) {
                                    paste_tracker::simulate_paste();
                                }
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

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

            if let Some(clip_id) = &id {
                let _ = state.db.log_paste(clip_id, &target_app);
            }

            if auto_paste.unwrap_or(false) {
                paste_tracker::simulate_paste();
            }
            return Ok(());
        }
    }

    #[cfg(not(windows))]
    {
        let target_text = if let Some(clip_id) = &id {
            if let Some(sensitive_str) = state.security.get_transient(clip_id) {
                sensitive_str
            } else {
                content
            }
        } else {
            content
        };

        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(target_text);
            if let Some(clip_id) = &id {
                let _ = state.db.log_paste(clip_id, "macOS Application");
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
fn toggle_notch(window: tauri::WebviewWindow) -> Result<(), String> {
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

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) -> Result<(), String> {
    let _ = window.hide();
    Ok(())
}

#[tauri::command]
fn show_window(window: tauri::WebviewWindow) -> Result<(), String> {
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

fn set_window_bounds(window: &tauri::WebviewWindow, width: f64, height: f64) {
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let monitor_size = monitor.size();
        let monitor_pos = monitor.position();
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let phys_width = (width * scale_factor) as i32;
        let x = monitor_pos.x + (monitor_size.width as i32 - phys_width) / 2;
        let y = monitor_pos.y;
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
    }
}

#[tauri::command]
fn shrink_to_pill(window: tauri::WebviewWindow) -> Result<(), String> {
    set_window_bounds(&window, 140.0, 28.0);
    Ok(())
}

#[tauri::command]
fn show_preview_notch(window: tauri::WebviewWindow) -> Result<(), String> {
    set_window_bounds(&window, 480.0, 32.0);
    Ok(())
}

#[tauri::command]
fn expand_window(window: tauri::WebviewWindow) -> Result<(), String> {
    set_window_bounds(&window, 700.0, 500.0);
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
fn collapse_window(window: tauri::WebviewWindow) -> Result<(), String> {
    set_window_bounds(&window, 140.0, 28.0);
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

            // 4. Position Window at Top-Center of Primary Monitor as Preview Notch
            if let Some(window) = app.get_webview_window("main") {
                set_window_bounds(&window, 480.0, 32.0);
            }

            // 5. System Tray Icon Setup
            let open_item = tauri::menu::MenuItem::with_id(app, "open", "Open Clipz (Alt+C)", true, None::<&str>)?;
            let hide_item = tauri::menu::MenuItem::with_id(app, "hide", "Hide Clipz", true, None::<&str>)?;
            let quit_item = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let tray_menu = tauri::menu::Menu::with_items(app, &[&open_item, &hide_item, &quit_item])?;

            if let Some(icon) = app.default_window_icon() {
                let _tray = tauri::tray::TrayIconBuilder::new()
                    .icon(icon.clone())
                    .menu(&tray_menu)
                    .on_menu_event(|app_handle, event| match event.id.as_ref() {
                        "open" => {
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "hide" => {
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.hide();
                            }
                        }
                        "quit" => {
                            app_handle.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = event {
                            let app_handle = tray.app_handle();
                            if let Some(window) = app_handle.get_webview_window("main") {
                                if let Ok(is_visible) = window.is_visible() {
                                    if is_visible {
                                        let _ = window.hide();
                                    } else {
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }
                                }
                            }
                        }
                    })
                    .build(app);
            }

            let handle_clip = handle.clone();
            let state_clip = Arc::clone(&app_state_clip);

            // Processing thread for Captured Clips
            std::thread::spawn(move || {
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

                    // Update active clip ID for paste tracking
                    *state_clip.active_clip_id.lock().unwrap() = Some(clip_id.clone());

                    // Emit real-time stream update to WebView Notch UI
                    let _ = handle_clip.emit("new-clip", &item);
                }
            });

            // Processing thread for Paste Tracking Events & Hotkeys
            let handle_paste = handle.clone();
            let state_paste = Arc::clone(&app_state_clip);
            std::thread::spawn(move || {
                while let Ok(paste_event) = paste_rx.recv() {
                    if paste_event.target_app == "HOTKEY_ALT_C" {
                        if let Some(window) = handle_paste.get_webview_window("main") {
                            if let Ok(is_visible) = window.is_visible() {
                                if is_visible {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                        let _ = handle_paste.emit("toggle-notch-hotkey", ());
                    } else {
                        let active_id = state_paste.active_clip_id.lock().unwrap().clone();
                        if let Some(ref clip_id) = active_id {
                            let _ = state_paste.db.log_paste(clip_id, &paste_event.target_app);
                        }
                        let _ = handle_paste.emit("paste-event", &paste_event);
                    }
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
            toggle_notch,
            hide_window,
            show_window,
            expand_window,
            collapse_window,
            shrink_to_pill,
            show_preview_notch,
            get_paste_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn classify_content(content: &str, is_sensitive: bool) -> String {
    if is_sensitive {
        return "sensitive".to_string();
    }
    let trimmed = content.trim();
    if trimmed.starts_with("data:image/") {
        return "image".to_string();
    }
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
