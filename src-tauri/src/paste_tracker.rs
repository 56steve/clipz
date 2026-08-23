use std::sync::mpsc::Sender;
use std::thread;

#[cfg(windows)]
use windows::{
    Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_V,
    },
    Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx,
        HHOOK, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    },
};

#[derive(Debug, Clone, serde::Serialize)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedShortcut {
    pub vk: u32,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

static CURRENT_SHORTCUT: std::sync::RwLock<Option<ParsedShortcut>> = std::sync::RwLock::new(None);

pub fn update_global_shortcut(shortcut_str: &str) {
    if let Ok(mut lock) = CURRENT_SHORTCUT.write() {
        if shortcut_str.trim().is_empty() {
            *lock = None;
        } else {
            *lock = parse_shortcut_str(shortcut_str);
        }
    }
}

fn parse_shortcut_str(s: &str) -> Option<ParsedShortcut> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut meta = false;
    let mut vk = 0u32;

    for part in parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" | "option" => alt = true,
            "shift" => shift = true,
            "cmd" | "meta" | "super" | "win" => meta = true,
            k => {
                vk = match k {
                    "space" => 0x20,
                    "tab" => 0x09,
                    "esc" | "escape" => 0x1B,
                    "enter" | "return" => 0x0D,
                    s if s.len() == 1 => {
                        let ch = s.chars().next().unwrap();
                        if ch >= 'a' && ch <= 'z' {
                            (ch as u8 - b'a' + b'A') as u32
                        } else if ch >= '0' && ch <= '9' {
                            ch as u32
                        } else {
                            0
                        }
                    }
                    _ => 0,
                };
            }
        }
    }

    if vk != 0 {
        Some(ParsedShortcut { vk, ctrl, alt, shift, meta })
    } else {
        None
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
        
        let shortcut = CURRENT_SHORTCUT.read().ok().and_then(|lock| lock.clone())
            .unwrap_or(ParsedShortcut { vk: 0x43, ctrl: false, alt: true, shift: false, meta: false });

        if kbd.vkCode == shortcut.vk {
            let alt_pressed = (kbd.flags.0 & 0x20 != 0) || ((GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0);
            let ctrl_pressed = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
            let shift_pressed = (GetAsyncKeyState(windows::Win32::UI::Input::KeyboardAndMouse::VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
            let meta_pressed = (GetAsyncKeyState(windows::Win32::UI::Input::KeyboardAndMouse::VK_LWIN.0 as i32) as u16 & 0x8000) != 0 
                || (GetAsyncKeyState(windows::Win32::UI::Input::KeyboardAndMouse::VK_RWIN.0 as i32) as u16 & 0x8000) != 0;

            if alt_pressed == shortcut.alt && ctrl_pressed == shortcut.ctrl && shift_pressed == shortcut.shift && meta_pressed == shortcut.meta {
                if let Some(ref tx) = PASTE_TX {
                    let _ = tx.send(PasteEvent {
                        target_app: "HOTKEY_ALT_C".to_string(),
                        timestamp: chrono_now_secs(),
                    });
                }
            }
        } else if kbd.vkCode == VK_V.0 as u32 {
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
pub fn simulate_paste() {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
        };

        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: Default::default(),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_V,
                        wScan: 0,
                        dwFlags: Default::default(),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_V,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(target_os = "macos")]
pub fn simulate_paste() {
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to keystroke \"v\" using command down")
        .output();
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn simulate_paste() {}

#[cfg(windows)]
pub fn get_active_app_name() -> String {
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

#[cfg(target_os = "macos")]
pub fn get_active_app_name() -> String {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first process whose frontmost is true")
        .output();
    if let Ok(out) = output {
        let app = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !app.is_empty() {
            return app;
        }
    }
    "macOS Application".to_string()
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn get_active_app_name() -> String {
    "Desktop".to_string()
}

fn chrono_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
