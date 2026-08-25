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

            #[cfg(target_os = "macos")]
            unsafe {
                macos_tracker::start_mac_tracking(tx);
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
        let clean = shortcut_str.trim().to_lowercase();
        if clean.is_empty() || clean == "disabled" || clean == "none" || clean == "off" {
            *lock = None;
        } else {
            *lock = parse_shortcut_str(shortcut_str);
        }
    }
}

fn parse_shortcut_str(s: &str) -> Option<ParsedShortcut> {
    let clean = s.trim();
    if clean.is_empty() || clean.eq_ignore_ascii_case("disabled") || clean.eq_ignore_ascii_case("none") {
        return None;
    }

    let parts: Vec<&str> = clean.split('+').map(|p| p.trim()).collect();
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
        
        let shortcut_opt = CURRENT_SHORTCUT.read().ok().and_then(|lock| lock.clone());

        if let Some(shortcut) = shortcut_opt {
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

#[cfg(target_os = "macos")]
mod macos_tracker {
    use super::*;
    use std::ffi::c_void;

    type CGEventRef = *mut c_void;
    type CGEventTapProxy = *mut c_void;
    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *mut c_void;

    type CGEventTapCallBack = unsafe extern "C" fn(
        proxy: CGEventTapProxy,
        type_: u32,
        event: CGEventRef,
        refcon: *mut c_void,
    ) -> CGEventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            eventsOfInterest: u64,
            callback: CGEventTapCallBack,
            refcon: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CFMachPortCreateRunLoopSource(
            allocator: *mut c_void,
            tap: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
        fn CGEventGetFlags(event: CGEventRef) -> u64;
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        static kCFRunLoopCommonModes: CFStringRef;
    }

    static mut MAC_PASTE_TX: Option<Sender<PasteEvent>> = None;

    pub unsafe fn start_mac_tracking(tx: Sender<PasteEvent>) {
        MAC_PASTE_TX = Some(tx);
        let event_mask = 1u64 << 10; // KeyDown
        let tap = CGEventTapCreate(0, 0, 0, event_mask, mac_keyboard_callback, std::ptr::null_mut());
        if !tap.is_null() {
            let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
            if !source.is_null() {
                let rl = CFRunLoopGetCurrent();
                CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                CFRunLoopRun();
            }
        }
    }

    unsafe extern "C" fn mac_keyboard_callback(
        _proxy: CGEventTapProxy,
        type_: u32,
        event: CGEventRef,
        _refcon: *mut c_void,
    ) -> CGEventRef {
        if type_ == 10 {
            let keycode = CGEventGetIntegerValueField(event, 9) as u32;
            let flags = CGEventGetFlags(event);

            let shortcut_opt = CURRENT_SHORTCUT.read().ok().and_then(|lock| lock.clone());
            if let Some(shortcut) = shortcut_opt {
                let mac_vk = mac_keycode_from_vk(shortcut.vk);
                let cmd_pressed = (flags & 0x00100000) != 0;
                let alt_pressed = (flags & 0x00080000) != 0;
                let ctrl_pressed = (flags & 0x00040000) != 0;
                let shift_pressed = (flags & 0x00020000) != 0;

                let meta_match = if shortcut.meta { cmd_pressed } else { !cmd_pressed };
                let alt_match = if shortcut.alt { alt_pressed } else { !alt_pressed };
                let ctrl_match = if shortcut.ctrl { ctrl_pressed } else { !ctrl_pressed };
                let shift_match = if shortcut.shift { shift_pressed } else { !shift_pressed };

                if keycode == mac_vk && meta_match && alt_match && ctrl_match && shift_match {
                    if let Some(ref tx) = MAC_PASTE_TX {
                        let _ = tx.send(PasteEvent {
                            target_app: "HOTKEY_ALT_C".to_string(),
                            timestamp: super::chrono_now_secs(),
                        });
                    }
                }
            }
        }
        event
    }

    fn mac_keycode_from_vk(vk: u32) -> u32 {
        match vk {
            0x43 => 8,  // 'C'
            0x56 => 9,  // 'V'
            0x41 => 0,  // 'A'
            0x42 => 11, // 'B'
            0x44 => 2,  // 'D'
            0x45 => 14, // 'E'
            0x46 => 3,  // 'F'
            0x47 => 5,  // 'G'
            0x48 => 4,  // 'H'
            0x49 => 34, // 'I'
            0x4A => 38, // 'J'
            0x4B => 40, // 'K'
            0x4C => 37, // 'L'
            0x4D => 46, // 'M'
            0x4E => 45, // 'N'
            0x4F => 31, // 'O'
            0x50 => 35, // 'P'
            0x51 => 12, // 'Q'
            0x52 => 15, // 'R'
            0x53 => 1,  // 'S'
            0x54 => 17, // 'T'
            0x55 => 32, // 'U'
            0x57 => 13, // 'W'
            0x58 => 7,  // 'X'
            0x59 => 16, // 'Y'
            0x5A => 6,  // 'Z'
            0x20 => 49, // Space
            _ => vk,
        }
    }
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
