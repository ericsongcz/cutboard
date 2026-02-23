use std::sync::OnceLock;
use tauri::Manager;

static HOTKEY_THREAD_ID: OnceLock<u32> = OnceLock::new();

const HOTKEY_ID: i32 = 9001;
const WM_REREGISTER: u32 = 0x0401;

#[cfg(debug_assertions)]
fn hk_log(msg: &str) {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("hotkey_debug.log");
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                use std::io::Write;
                let _ = writeln!(
                    f,
                    "[{}] {}",
                    chrono::Local::now().format("%H:%M:%S%.3f"),
                    msg
                );
            }
        }
    }
}

#[cfg(not(debug_assertions))]
fn hk_log(_msg: &str) {}

pub fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() {
        return None;
    }

    let mut mod_flags: u32 = 0x4000; // MOD_NOREPEAT
    let mut key_part = "";

    for part in &parts {
        match part.trim() {
            "Alt" => mod_flags |= 0x0001,
            "Ctrl" | "Control" => mod_flags |= 0x0002,
            "Shift" => mod_flags |= 0x0004,
            "Super" | "Meta" | "Win" => mod_flags |= 0x0008,
            k => key_part = k,
        }
    }

    let vk: u32 = if key_part.len() == 1 {
        let c = key_part.chars().next()?;
        if c.is_ascii_alphabetic() {
            c.to_ascii_uppercase() as u32
        } else if c.is_ascii_digit() {
            c as u32
        } else {
            return None;
        }
    } else {
        match key_part {
            "F1" => 0x70,
            "F2" => 0x71,
            "F3" => 0x72,
            "F4" => 0x73,
            "F5" => 0x74,
            "F6" => 0x75,
            "F7" => 0x76,
            "F8" => 0x77,
            "F9" => 0x78,
            "F10" => 0x79,
            "F11" => 0x7A,
            "F12" => 0x7B,
            "Space" => 0x20,
            "Enter" => 0x0D,
            "Tab" => 0x09,
            "Escape" => 0x1B,
            _ => return None,
        }
    };

    if mod_flags & 0x000F == 0 {
        return None;
    }

    Some((mod_flags, vk))
}

pub fn start(app: tauri::AppHandle, shortcut: &str) {
    hk_log(&format!("start() called with shortcut='{}'", shortcut));

    let (mod_flags, vk) = match parse_hotkey(shortcut) {
        Some(v) => {
            hk_log(&format!(
                "parse_hotkey OK: mod=0x{:04x}, vk=0x{:02x}",
                v.0, v.1
            ));
            v
        }
        None => {
            hk_log(&format!("parse_hotkey FAILED for '{}'", shortcut));
            return;
        }
    };

    #[cfg(windows)]
    std::thread::spawn(move || {
        hk_log("hotkey thread started");
        run_hotkey_loop(app, mod_flags, vk);
        hk_log("hotkey thread EXITED (unexpected)");
    });

    #[cfg(not(windows))]
    let _ = (app, mod_flags, vk);
}

#[cfg(windows)]
fn run_hotkey_loop(app: tauri::AppHandle, initial_mod: u32, initial_vk: u32) {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    std::thread::sleep(std::time::Duration::from_millis(500));

    unsafe {
        let tid = GetCurrentThreadId();
        HOTKEY_THREAD_ID.set(tid).ok();
        hk_log(&format!("thread id={}, starting registration", tid));

        let mut registered = false;
        for attempt in 0..20 {
            match RegisterHotKey(
                None,
                HOTKEY_ID,
                HOT_KEY_MODIFIERS(initial_mod),
                initial_vk,
            ) {
                Ok(_) => {
                    hk_log(&format!("RegisterHotKey OK on attempt {}", attempt + 1));
                    registered = true;
                    break;
                }
                Err(e) => {
                    hk_log(&format!(
                        "RegisterHotKey attempt {} FAILED: {:?}",
                        attempt + 1,
                        e
                    ));
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }

        if !registered {
            hk_log("GIVING UP after 20 attempts");
        }

        hk_log("entering GetMessageW loop");
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == 0 || ret.0 == -1 {
                break;
            }
            if msg.message == WM_HOTKEY {
                hk_log("WM_HOTKEY received, toggling window");
                toggle_window(&app);
            } else if msg.message == WM_REREGISTER {
                hk_log("WM_REREGISTER received");
                let _ = UnregisterHotKey(None, HOTKEY_ID);
                let new_mod = msg.wParam.0 as u32;
                let new_vk = msg.lParam.0 as u32;
                for attempt in 0..5 {
                    if RegisterHotKey(
                        None,
                        HOTKEY_ID,
                        HOT_KEY_MODIFIERS(new_mod),
                        new_vk,
                    )
                    .is_ok()
                    {
                        hk_log(&format!(
                            "re-register OK on attempt {} (mod=0x{:04x}, vk=0x{:02x})",
                            attempt + 1,
                            new_mod,
                            new_vk
                        ));
                        break;
                    }
                    hk_log(&format!("re-register attempt {} failed", attempt + 1));
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            } else {
                hk_log(&format!("other msg: 0x{:04x}", msg.message));
            }
        }
        hk_log("GetMessageW loop ended");
    }
}

fn toggle_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::*;

            let hwnd = match window.hwnd() {
                Ok(h) => HWND(h.0),
                Err(_) => {
                    hk_log("toggle: failed to get hwnd");
                    return;
                }
            };

            unsafe {
                let visible = IsWindowVisible(hwnd).as_bool();
                let fg = GetForegroundWindow();
                let is_foreground = fg == hwnd;

                hk_log(&format!(
                    "toggle(win32): visible={}, is_foreground={}",
                    visible, is_foreground
                ));

                if visible && is_foreground {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
        }

        #[cfg(not(windows))]
        {
            let visible = window.is_visible().unwrap_or(false);
            let focused = window.is_focused().unwrap_or(false);
            if visible && focused {
                let _ = window.hide();
            } else {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
    } else {
        hk_log("toggle: main window NOT FOUND");
    }
}

pub fn update(new_shortcut: &str) {
    hk_log(&format!("update() called with '{}'", new_shortcut));

    #[cfg(windows)]
    {
        if let (Some(&tid), Some((mod_flags, vk))) =
            (HOTKEY_THREAD_ID.get(), parse_hotkey(new_shortcut))
        {
            use windows::Win32::Foundation::LPARAM;
            use windows::Win32::Foundation::WPARAM;
            use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
            unsafe {
                let _ = PostThreadMessageW(
                    tid,
                    WM_REREGISTER,
                    WPARAM(mod_flags as usize),
                    LPARAM(vk as isize),
                );
            }
            hk_log(&format!(
                "PostThreadMessageW sent to tid={} (mod=0x{:04x}, vk=0x{:02x})",
                tid, mod_flags, vk
            ));
        } else {
            hk_log("update: HOTKEY_THREAD_ID or parse failed");
        }
    }

    #[cfg(not(windows))]
    let _ = new_shortcut;
}
