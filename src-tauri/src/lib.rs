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
            if target_text.starts_with("data:image/") {
                if let Some(pos) = target_text.find(";base64,") {
                    let b64_str = &target_text[pos + 8..];
                    if let Ok(bmp_bytes) = clipboard::base64_decode(b64_str) {
                        if bmp_bytes.len() > 54 && &bmp_bytes[0..2] == b"BM" {
                            let width = u32::from_le_bytes(bmp_bytes[18..22].try_into().unwrap_or_default()) as usize;
                            let raw_height = i32::from_le_bytes(bmp_bytes[22..26].try_into().unwrap_or_default());
                            let height = raw_height.abs() as usize;
                            let off_bits = u32::from_le_bytes(bmp_bytes[10..14].try_into().unwrap_or_default()) as usize;

                            if off_bits < bmp_bytes.len() && width > 0 && height > 0 {
                                let pixel_bytes = &bmp_bytes[off_bits..];
                                let mut rgba_pixels = Vec::with_capacity(pixel_bytes.len());
                                for chunk in pixel_bytes.chunks_exact(4) {
                                    rgba_pixels.push(chunk[2]); // R
                                    rgba_pixels.push(chunk[1]); // G
                                    rgba_pixels.push(chunk[0]); // B
                                    rgba_pixels.push(chunk[3]); // A
                                }
                                let img_data = arboard::ImageData {
                                    width,
                                    height,
                                    bytes: std::borrow::Cow::Owned(rgba_pixels),
                                };
                                let _ = clipboard.set_image(img_data);

                                if let Some(clip_id) = &id {
                                    let _ = state.db.log_paste(clip_id, &paste_tracker::get_active_app_name());
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

            let _ = clipboard.set_text(target_text);
            if let Some(clip_id) = &id {
                let _ = state.db.log_paste(clip_id, &paste_tracker::get_active_app_name());
            }
            if auto_paste.unwrap_or(false) {
                paste_tracker::simulate_paste();
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
fn set_clip_reminder(
    state: State<'_, Arc<AppState>>,
    id: String,
    reminder_at: Option<i64>,
) -> Result<(), String> {
    state.db.set_clip_reminder(&id, reminder_at).map_err(|e| e.to_string())
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

fn resize_window_preserving_position(window: &tauri::WebviewWindow, logical_width: f64, logical_height: f64) {
    let _ = window.set_shadow(false);
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let target_phys_width = (logical_width * scale_factor).round() as i32;
    let target_phys_height = (logical_height * scale_factor).round() as i32;

    let current_pos = window.outer_position().ok();
    let current_size = window.outer_size().ok();

    if let (Some(pos), Some(size)) = (current_pos, current_size) {
        let center_x = pos.x + (size.width as i32) / 2;
        let mut new_x = center_x - target_phys_width / 2;
        let mut new_y = pos.y;

        let monitor = window.current_monitor().ok().flatten().or_else(|| window.primary_monitor().ok().flatten());
        if let Some(mon) = monitor {
            let mon_pos = mon.position();
            let mon_size = mon.size();
            let min_x = mon_pos.x;
            let max_x = mon_pos.x + mon_size.width as i32 - target_phys_width;
            let min_y = mon_pos.y;
            let max_y = mon_pos.y + mon_size.height as i32 - target_phys_height;

            if max_x >= min_x {
                new_x = new_x.clamp(min_x, max_x);
            }
            if max_y >= min_y {
                new_y = new_y.clamp(min_y, max_y);
            }
        }

        #[cfg(windows)]
        if let Ok(hwnd) = window.hwnd() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE};
            unsafe {
                let _ = SetWindowPos(
                    HWND(hwnd.0),
                    HWND_TOPMOST,
                    new_x,
                    new_y,
                    target_phys_width,
                    target_phys_height,
                    SWP_NOACTIVATE,
                );
            }
            return;
        }

        #[cfg(not(windows))]
        {
            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(logical_width, logical_height)));
            let logical_x = (new_x as f64) / scale_factor;
            let logical_y = (new_y as f64) / scale_factor;
            let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(logical_x, logical_y)));
            let _ = window.set_shadow(false);
        }
    } else if let Ok(Some(mon)) = window.primary_monitor() {
        let mon_pos = mon.position();
        let mon_size = mon.size();
        let x = mon_pos.x + (mon_size.width as i32 - target_phys_width) / 2;
        let y = mon_pos.y + (12.0 * scale_factor).round() as i32;

        #[cfg(windows)]
        if let Ok(hwnd) = window.hwnd() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE};
            unsafe {
                let _ = SetWindowPos(
                    HWND(hwnd.0),
                    HWND_TOPMOST,
                    x,
                    y,
                    target_phys_width,
                    target_phys_height,
                    SWP_NOACTIVATE,
                );
            }
            return;
        }

        #[cfg(not(windows))]
        {
            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(logical_width, logical_height)));
            let logical_x = (x as f64) / scale_factor;
            let logical_y = (y as f64) / scale_factor;
            let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(logical_x, logical_y)));
            let _ = window.set_shadow(false);
        }
    }
}

fn initial_center_position(window: &tauri::WebviewWindow, logical_width: f64, logical_height: f64) {
    let _ = window.set_shadow(false);
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let target_phys_width = (logical_width * scale_factor).round() as i32;
    let target_phys_height = (logical_height * scale_factor).round() as i32;

    if let Ok(Some(mon)) = window.primary_monitor() {
        let mon_pos = mon.position();
        let mon_size = mon.size();
        let x = mon_pos.x + (mon_size.width as i32 - target_phys_width) / 2;
        let y = mon_pos.y + (12.0 * scale_factor).round() as i32;

        #[cfg(windows)]
        if let Ok(hwnd) = window.hwnd() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE};
            unsafe {
                let _ = SetWindowPos(
                    HWND(hwnd.0),
                    HWND_TOPMOST,
                    x,
                    y,
                    target_phys_width,
                    target_phys_height,
                    SWP_NOACTIVATE,
                );
            }
            return;
        }

        #[cfg(not(windows))]
        {
            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(logical_width, logical_height)));
            let logical_x = (x as f64) / scale_factor;
            let logical_y = (y as f64) / scale_factor;
            let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(logical_x, logical_y)));
        }
    }
}

#[tauri::command]
fn shrink_to_pill(window: tauri::WebviewWindow) -> Result<(), String> {
    resize_window_preserving_position(&window, 130.0, 36.0);
    Ok(())
}

#[tauri::command]
fn show_preview_notch(window: tauri::WebviewWindow) -> Result<(), String> {
    resize_window_preserving_position(&window, 500.0, 46.0);
    Ok(())
}

#[tauri::command]
fn expand_window(window: tauri::WebviewWindow) -> Result<(), String> {
    resize_window_preserving_position(&window, 700.0, 520.0);
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
fn start_dragging(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
fn collapse_window(window: tauri::WebviewWindow) -> Result<(), String> {
    resize_window_preserving_position(&window, 130.0, 36.0);
    Ok(())
}

#[tauri::command]
fn get_global_shortcut(state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    if let Ok(Some(sc)) = state.db.get_setting("global_shortcut") {
        Ok(sc)
    } else {
        #[cfg(target_os = "macos")]
        return Ok("Cmd+Shift+C".to_string());
        #[cfg(not(target_os = "macos"))]
        return Ok("Alt+C".to_string());
    }
}

#[tauri::command]
fn center_window(window: tauri::WebviewWindow) -> Result<(), String> {
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let win_width = (130.0 * scale) as u32;
        let x = ((size.width.saturating_sub(win_width)) / 2) as i32;
        let y = 12;
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    }
    Ok(())
}

#[tauri::command]
fn save_global_shortcut(shortcut: String, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let clean = shortcut.trim();
    if clean.is_empty() || clean.eq_ignore_ascii_case("disabled") || clean.eq_ignore_ascii_case("none") {
        state.db.set_setting("global_shortcut", "Disabled").map_err(|e| e.to_string())?;
        paste_tracker::update_global_shortcut("Disabled");
        return Ok(());
    }
    state.db.set_setting("global_shortcut", clean).map_err(|e| e.to_string())?;
    paste_tracker::update_global_shortcut(clean);
    Ok(())
}

#[tauri::command]
fn disable_and_uninstall_app(
    app_handle: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
    clean_data: Option<bool>,
) -> Result<(), String> {
    // 1. Disable hotkeys globally
    paste_tracker::update_global_shortcut("Disabled");
    let _ = state.db.set_setting("global_shortcut", "Disabled");

    // 2. Remove platform-specific autostart entries
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("reg")
            .args(&["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "Clipz", "/f"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output();
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let plist_path = std::path::Path::new(&home)
                .join("Library/LaunchAgents/com.antigravity.clipz.plist");
            if plist_path.exists() {
                let _ = std::fs::remove_file(plist_path);
            }
        }
    }

    // 3. Optional clean DB and local data
    if clean_data.unwrap_or(false) {
        let _ = state.db.clear_all_clips();
    }

    // 4. Exit app cleanly
    let handle = app_handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        handle.exit(0);
    });

    Ok(())
}

pub fn run() {
    let db = DatabaseManager::new().expect("Failed to initialize SQLite database");
    let security = SecurityManager::new();

    let initial_shortcut = db.get_setting("global_shortcut").ok().flatten().unwrap_or_else(|| {
        if cfg!(target_os = "macos") { "Cmd+Shift+C".to_string() } else { "Alt+C".to_string() }
    });
    paste_tracker::update_global_shortcut(&initial_shortcut);

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

            // 4. Position Window at Top-Center of Primary Monitor as Compact Pill
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_shadow(false);
                initial_center_position(&window, 130.0, 36.0);

                #[cfg(target_os = "macos")]
                {
                    use cocoa::base::{id, nil};
                    use cocoa::foundation::NSString;
                    use objc::{msg_send, sel, sel_impl, class};
                    let _ = window.with_webview(|webview| unsafe {
                        let ns_view: id = webview.inner() as id;
                        let key = NSString::alloc(nil).init_str("drawsBackground");
                        let no: id = msg_send![class!(NSNumber), numberWithBool: false];
                        let _: () = msg_send![ns_view, setValue: no forKey: key];
                    });
                }
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
                    let clip_id = format!("{:x}", md5_hash(&raw_event.content));
                    let is_sensitive = SecurityManager::is_sensitive_source(&raw_event.source_app)
                        || SecurityManager::is_sensitive_pattern(&raw_event.content);

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
                        reminder_at: None,
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
            set_clip_reminder,
            reveal_sensitive,
            toggle_notch,
            hide_window,
            show_window,
            expand_window,
            collapse_window,
            shrink_to_pill,
            show_preview_notch,
            start_dragging,
            get_paste_history,
            get_global_shortcut,
            save_global_shortcut,
            center_window,
            disable_and_uninstall_app
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
