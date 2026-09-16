#[cfg(windows)]
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

#[cfg(windows)]
use windows::{
    Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, GetClipboardSequenceNumber,
        IsClipboardFormatAvailable, OpenClipboard,
    },
    Win32::System::Memory::GlobalLock,
    Win32::System::ProcessStatus::GetModuleFileNameExW,
    Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    Win32::UI::WindowsAndMessaging::{DefWindowProcW, GetForegroundWindow, GetWindowThreadProcessId},
};

fn hash_content(s: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for &b in s.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

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
            let mut last_captured_text = String::new();
            let mut last_captured_img_hash: u64 = 0;
            #[cfg(windows)]
            let mut last_seq: u32 = unsafe { GetClipboardSequenceNumber() };

            loop {
                thread::sleep(std::time::Duration::from_millis(150));

                // Track who is in front BEFORE anything is copied: once the
                // notch opens, Clipz is frontmost and the real answer is gone.
                crate::paste_tracker::remember_foreground();

                #[cfg(windows)]
                {
                    let current_seq = unsafe { GetClipboardSequenceNumber() };
                    if current_seq != 0 && current_seq == last_seq {
                        continue;
                    }
                    last_seq = current_seq;
                }

                let mut captured = false;
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() && text != last_captured_text {
                            last_captured_text = text.clone();
                            let source_app = get_active_app_name();
                            let _ = tx.send(RawClipEvent {
                                content: text,
                                source_app,
                                is_image: false,
                            });
                            captured = true;
                        }
                    } else if let Ok(image) = clipboard.get_image() {
                        if !image.bytes.is_empty() {
                            let bmp_base64 = rgba_to_bmp_base64(&image);
                            let img_hash = hash_content(&bmp_base64);
                            if img_hash != 0 && img_hash != last_captured_img_hash {
                                last_captured_img_hash = img_hash;
                                let source_app = get_active_app_name();
                                let _ = tx.send(RawClipEvent {
                                    content: bmp_base64,
                                    source_app,
                                    is_image: true,
                                });
                                captured = true;
                            }
                        }
                    }
                }

                #[cfg(windows)]
                if !captured {
                    if let Some(text) = read_clipboard_text() {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() && text != last_captured_text {
                            last_captured_text = text.clone();
                            let source_app = get_active_app_name();
                            let _ = tx.send(RawClipEvent {
                                content: text,
                                source_app,
                                is_image: false,
                            });
                        }
                    } else if let Some(img_data) = read_clipboard_image() {
                        if !img_data.is_empty() {
                            let img_hash = hash_content(&img_data);
                            if img_hash != 0 && img_hash != last_captured_img_hash {
                                last_captured_img_hash = img_hash;
                                let source_app = get_active_app_name();
                                let _ = tx.send(RawClipEvent {
                                    content: img_data,
                                    source_app,
                                    is_image: true,
                                });
                            }
                        }
                    }
                }
            }
        });
    }
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

        // Micro-retry loop (5 attempts x 10ms) if source application locks clipboard briefly
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(HWND::default()).is_ok() {
                opened = true;
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }

        if !opened {
            return None;
        }

        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_err() {
            let _ = CloseClipboard();
            return None;
        }

        let hmem = windows::Win32::Foundation::HGLOBAL(handle.unwrap().0);
        let ptr = GlobalLock(hmem);
        if ptr.is_null() {
            let _ = CloseClipboard();
            return None;
        }

        let size = windows::Win32::System::Memory::GlobalSize(hmem);
        if size < 2 {
            let _ = windows::Win32::System::Memory::GlobalUnlock(hmem);
            let _ = CloseClipboard();
            return None;
        }

        let num_u16 = size / 2;
        let slice = std::slice::from_raw_parts(ptr as *const u16, num_u16);
        let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
        let text = String::from_utf16_lossy(&slice[..len]);

        let _ = windows::Win32::System::Memory::GlobalUnlock(hmem);
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

        // Micro-retry loop (5 attempts x 10ms) if source application locks clipboard briefly
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(HWND::default()).is_ok() {
                opened = true;
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }

        if !opened {
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

pub fn rgba_to_bmp_base64(img: &arboard::ImageData) -> String {
    let width = img.width as u32;
    let height = img.height as u32;
    let rgba_bytes = &img.bytes;

    let pixel_count = (width * height) as usize;
    let mut bgra_pixels = Vec::with_capacity(pixel_count * 4);

    for chunk in rgba_bytes.chunks_exact(4) {
        bgra_pixels.push(chunk[2]); // B
        bgra_pixels.push(chunk[1]); // G
        bgra_pixels.push(chunk[0]); // R
        bgra_pixels.push(chunk[3]); // A
    }

    let header_size = 54u32;
    let image_size = (width * height * 4) as u32;
    let file_size = header_size + image_size;

    let mut bmp = Vec::with_capacity(file_size as usize);
    // BITMAPFILEHEADER (14 bytes)
    bmp.extend_from_slice(&0x4D42u16.to_le_bytes()); // 'BM'
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes());
    bmp.extend_from_slice(&header_size.to_le_bytes());

    // BITMAPINFOHEADER (40 bytes)
    bmp.extend_from_slice(&40u32.to_le_bytes()); // biSize
    bmp.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    bmp.extend_from_slice(&(-(height as i32)).to_le_bytes()); // biHeight (negative for top-down)
    bmp.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    bmp.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    bmp.extend_from_slice(&0u32.to_le_bytes()); // biCompression
    bmp.extend_from_slice(&image_size.to_le_bytes()); // biSizeImage
    bmp.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    bmp.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    bmp.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    bmp.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    bmp.extend_from_slice(&bgra_pixels);

    let encoded = base64_encode(&bmp);
    format!("data:image/bmp;base64,{}", encoded)
}

/// Who the clip was copied FROM, on every platform.
///
/// This file used to carry its own copy of the Win32 foreground-window lookup,
/// duplicating `paste_tracker`'s. That copy did not go through the
/// "never blame Clipz itself" filter, so on Windows every clip taken while the
/// notch had focus was still filed as clipz.exe. One path now, for both
/// platforms.
fn get_active_app_name() -> String {
    crate::paste_tracker::source_app_name()
}

