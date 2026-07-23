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

                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool()
                    || GetMessageW_blocking(&mut msg)
                {
                    if msg.message == WM_CLIPBOARDUPDATE {
                        if let Some(text) = read_clipboard_text() {
                            if !text.is_empty() && text != last_captured_text {
                                last_captured_text = text.clone();
                                let source_app = get_active_app_name();
                                let _ = tx.send(RawClipEvent {
                                    content: text,
                                    source_app,
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
unsafe fn GetMessageW_blocking(msg: *mut MSG) -> bool {
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
