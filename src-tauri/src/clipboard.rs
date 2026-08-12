use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Win32::System::DataExchange::{
        AddClipboardFormatListener, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, RemoveClipboardFormatListener,
    },
    Win32::System::Memory::GlobalLock,
    Win32::System::ProcessStatus::GetModuleFileNameExW,
    Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow,
        GetWindowThreadProcessId, PeekMessageW, RegisterClassW, HWND_MESSAGE, MSG, PM_REMOVE,
        WM_CLIPBOARDUPDATE, WM_DESTROY, WNDCLASSW,
    },
};

#[derive(Debug, Clone)]
pub struct RawClipEvent {
    pub content: String,
    pub source_app: String,
    pub is_image: bool,
}

pub struct ClipboardListener;

impl ClipboardListener {
    pub fn start_listening(tx: Sender<RawClipEvent>) {
        thread::spawn(move || {
            #[cfg(windows)]
            unsafe {
                let class_name = windows::core::w!("ClipzClipboardListenerClass");
                let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null())
                    .unwrap_or_default();

                let wnd_class = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance.into(),
                    lpszClassName: class_name,
                    ..Default::default()
                };

                RegisterClassW(&wnd_class);

                let hwnd = match CreateWindowExW(
                    Default::default(),
                    class_name,
                    windows::core::w!("ClipzClipboardListenerWindow"),
                    Default::default(),
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    None,
                    instance,
                    None,
                ) {
                    Ok(h) => h,
                    Err(_) => {
                        eprintln!("[Clipz] Failed to create hidden message window for clipboard listener");
                        return;
                    }
                };

                if AddClipboardFormatListener(hwnd).is_err() {
                    eprintln!("[Clipz] Failed to register AddClipboardFormatListener");
                    return;
                }

                let mut last_captured_text = String::new();
                let mut last_captured_img_len = 0;

                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool()
                    || get_message_w_blocking(&mut msg)
                {
                    if msg.message == WM_CLIPBOARDUPDATE {
                        let source_app = get_active_app_name();

                        // Check text first
                        if let Some(text) = read_clipboard_text() {
                            if !text.is_empty() && text != last_captured_text {
                                last_captured_text = text.clone();
                                let _ = tx.send(RawClipEvent {
                                    content: text,
                                    source_app: source_app.clone(),
                                    is_image: false,
                                });
                            }
                        } else if let Some(img_data) = read_clipboard_image() {
                            if img_data.len() != last_captured_img_len {
                                last_captured_img_len = img_data.len();
                                let _ = tx.send(RawClipEvent {
                                    content: img_data,
                                    source_app: source_app.clone(),
                                    is_image: true,
                                });
                            }
                        }
                    } else if msg.message == WM_DESTROY {
                        let _ = RemoveClipboardFormatListener(hwnd);
                        break;
                    }
                    DispatchMessageW(&msg);
                }
            }
        });
    }
}

#[cfg(windows)]
unsafe fn get_message_w_blocking(msg: *mut MSG) -> bool {
    windows::Win32::UI::WindowsAndMessaging::GetMessageW(msg, HWND::default(), 0, 0).as_bool()
}

#[cfg(windows)]
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[cfg(windows)]
fn read_clipboard_text() -> Option<String> {
    unsafe {
        const CF_UNICODETEXT: u32 = 13;
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return None;
        }

        if OpenClipboard(HWND::default()).is_err() {
            return None;
        }

        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_err() {
            let _ = CloseClipboard();
            return None;
        }

        let handle = handle.unwrap();
        let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0));
        if ptr.is_null() {
            let _ = CloseClipboard();
            return None;
        }

        let slice = std::slice::from_raw_parts(ptr as *const u16, 50000);
        let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
        let text = String::from_utf16_lossy(&slice[..len]);

        let _ = windows::Win32::System::Memory::GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
        let _ = CloseClipboard();

        Some(text)
    }
}

#[cfg(windows)]
fn read_clipboard_image() -> Option<String> {
    unsafe {
        const CF_DIB: u32 = 8;
        if IsClipboardFormatAvailable(CF_DIB).is_err() {
            return None;
        }

        if OpenClipboard(HWND::default()).is_err() {
            return None;
        }

        let handle = GetClipboardData(CF_DIB);
        if handle.is_err() {
            let _ = CloseClipboard();
            return None;
        }

        let handle = handle.unwrap();
        let hmem = windows::Win32::Foundation::HGLOBAL(handle.0);
        let size = windows::Win32::System::Memory::GlobalSize(hmem);
        if size == 0 {
            let _ = CloseClipboard();
            return None;
        }

        let ptr = GlobalLock(hmem);
        if ptr.is_null() {
            let _ = CloseClipboard();
            return None;
        }

        let dib_slice = std::slice::from_raw_parts(ptr as *const u8, size);

        if dib_slice.len() < 40 {
            let _ = windows::Win32::System::Memory::GlobalUnlock(hmem);
            let _ = CloseClipboard();
            return None;
        }

        let header_size = u32::from_le_bytes(dib_slice[0..4].try_into().unwrap()) as usize;
        let bit_count = u16::from_le_bytes(dib_slice[14..16].try_into().unwrap());
        let clr_used = u32::from_le_bytes(dib_slice[32..36].try_into().unwrap());

        let num_colors = if clr_used != 0 {
            clr_used as usize
        } else if bit_count <= 8 {
            1 << bit_count
        } else {
            0
        };

        let palette_size = num_colors * 4;
        let off_bits = (14 + header_size + palette_size) as u32;
        let file_size = (14 + size) as u32;

        let mut bmp = Vec::with_capacity(14 + size);
        bmp.extend_from_slice(&0x4D42u16.to_le_bytes()); // 'BM'
        bmp.extend_from_slice(&file_size.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes());
        bmp.extend_from_slice(&off_bits.to_le_bytes());
        bmp.extend_from_slice(dib_slice);

        let _ = windows::Win32::System::Memory::GlobalUnlock(hmem);
        let _ = CloseClipboard();

        let encoded = base64_encode(&bmp);
        Some(format!("data:image/bmp;base64,{}", encoded))
    }
}

pub fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARSET[((triple >> 18) & 63) as usize] as char);
        result.push(CHARSET[((triple >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARSET[((triple >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARSET[(triple & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut buffer = 0u32;
    let mut bits = 0;
    let mut out = Vec::with_capacity(input.len() * 3 / 4);

    for &b in input.as_bytes() {
        if b == b'=' || b == b'\r' || b == b'\n' || b == b' ' {
            continue;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("Invalid base64 byte".to_string()),
        };
        buffer = (buffer << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(windows)]
fn get_active_app_name() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return "Unknown App".to_string();
        }

        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        if process_id == 0 {
            return "Unknown App".to_string();
        }

        let process_handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            process_id,
        );

        if let Ok(handle) = process_handle {
            let mut buf = [0u16; 1024];
            let len = GetModuleFileNameExW(handle, None, &mut buf);
            let _ = windows::Win32::Foundation::CloseHandle(handle);

            if len > 0 {
                let full_path = String::from_utf16_lossy(&buf[..len as usize]);
                if let Some(filename) = Path::new(&full_path).file_name() {
                    return filename.to_string_lossy().to_string();
                }
            }
        }

        "Unknown App".to_string()
    }
}

#[cfg(not(windows))]
fn get_active_app_name() -> String {
    "Desktop".to_string()
}
