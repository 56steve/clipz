use std::sync::mpsc::Sender;
use std::thread;

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_V,
    },
    Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx,
        HHOOK, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    },
};

#[derive(Debug, Clone)]
pub struct PasteEvent {
    pub target_app: String,
    pub timestamp: i64,
}

pub struct PasteTracker;

impl PasteTracker {
    pub fn start_tracking(tx: Sender<PasteEvent>) {
        thread::spawn(move || {
            #[cfg(windows)]
            unsafe {
                let hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(keyboard_proc),
                    HINSTANCE::default(),
                    0,
                );

                if let Ok(hook_handle) = hook {
                    PASTE_TX = Some(tx);
                    HOOK_HANDLE = Some(hook_handle);

                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {}

                    let _ = UnhookWindowsHookEx(hook_handle);
                } else {
                    eprintln!("[Clipz] Failed to install keyboard hook for paste tracking");
                }
            }
        });
    }
}

#[cfg(windows)]
static mut PASTE_TX: Option<Sender<PasteEvent>> = None;

#[cfg(windows)]
static mut HOOK_HANDLE: Option<HHOOK> = None;

#[cfg(windows)]
unsafe extern "system" fn keyboard_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode >= 0 && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN) {
        let kbd = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        if kbd.vkCode == VK_V.0 as u32 {
            let ctrl_pressed = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
            if ctrl_pressed {
                let target_app = get_active_app_name();
                if let Some(ref tx) = PASTE_TX {
                    let _ = tx.send(PasteEvent {
                        target_app,
                        timestamp: chrono_now_secs(),
                    });
                }
            }
        }
    }

    let hook_val = HOOK_HANDLE.unwrap_or_default();
    CallNextHookEx(hook_val, ncode, wparam, lparam)
}

#[cfg(windows)]
fn get_active_app_name() -> String {
    unsafe {
        let hwnd = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        if hwnd.0.is_null() {
            return "Unknown App".to_string();
        }

        let mut process_id: u32 = 0;
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        if process_id == 0 {
            return "Unknown App".to_string();
        }

        let process_handle = windows::Win32::System::Threading::OpenProcess(
            windows::Win32::System::Threading::PROCESS_QUERY_INFORMATION
                | windows::Win32::System::Threading::PROCESS_VM_READ,
            false,
            process_id,
        );

        if let Ok(handle) = process_handle {
            let mut buf = [0u16; 1024];
            let len = windows::Win32::System::ProcessStatus::GetModuleFileNameExW(handle, None, &mut buf);
            let _ = windows::Win32::Foundation::CloseHandle(handle);

            if len > 0 {
                let full_path = String::from_utf16_lossy(&buf[..len as usize]);
                if let Some(filename) = std::path::Path::new(&full_path).file_name() {
                    return filename.to_string_lossy().to_string();
                }
            }
        }

        "Unknown App".to_string()
    }
}

fn chrono_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
