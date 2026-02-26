mod clipboard;
mod commands;
mod config;
mod database;
pub mod hotkey;
mod sensitive;
mod window_tracker;

use chrono::Timelike;
use config::AppConfig;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

pub struct DbState(pub Arc<Mutex<database::Database>>);
pub struct ConfigPath(pub std::path::PathBuf);
struct TrayState(#[allow(dead_code)] tauri::tray::TrayIcon);

static LOG_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

fn setup_crash_handler(log_dir: &std::path::Path) {
    std::fs::create_dir_all(log_dir).ok();
    LOG_DIR.set(log_dir.to_path_buf()).ok();

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = LOG_DIR.get() {
            let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let path = dir.join(format!("crash_{}.log", ts));

            let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
            let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };

            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("<unnamed>");

            let content = format!(
                "CutBoard Crash Report\n\
                 ======================\n\
                 Time: {}\n\
                 Thread: {}\n\
                 Location: {}\n\
                 Message: {}\n\
                 Version: {}\n\
                 OS: {} {}\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                thread_name,
                location,
                payload,
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
            );
            std::fs::write(&path, content).ok();
        }
        prev(info);
    }));
}

fn check_last_crash(log_dir: &std::path::Path) -> Option<String> {
    let entries = std::fs::read_dir(log_dir).ok()?;
    let mut latest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("crash_") && name_str.ends_with(".log") {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if latest.as_ref().map_or(true, |(t, _)| modified > *t) {
                        latest = Some((modified, entry.path()));
                    }
                }
            }
        }
    }
    let (time, path) = latest?;
    let elapsed = std::time::SystemTime::now().duration_since(time).unwrap_or_default();
    if elapsed.as_secs() > 7 * 24 * 3600 {
        return None;
    }
    let filename = path.file_name()?.to_string_lossy().to_string();
    Some(filename)
}

pub fn run() {
    #[cfg(windows)]
    {
        if !acquire_single_instance_lock() {
            activate_existing_instance();
            return;
        }
    }

    #[cfg(windows)]
    unsafe {
        use windows::core::w;
        let _ =
            windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(w!("CutBoard"));
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let default_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&default_data_dir)?;

            let config_path = AppConfig::config_file_path(&default_data_dir);
            let mut cfg = AppConfig::load(&config_path);

            let mut need_save = false;
            if cfg.data_path.is_empty() {
                cfg.data_path = default_data_dir.to_string_lossy().to_string();
                need_save = true;
            }

            let mut data_dir = std::path::PathBuf::from(&cfg.data_path);
            if let Err(_) = std::fs::create_dir_all(&data_dir) {
                eprintln!("Cannot access data_path '{}', falling back to default", cfg.data_path);
                cfg.data_path = default_data_dir.to_string_lossy().to_string();
                data_dir = default_data_dir.clone();
                need_save = true;
                std::fs::create_dir_all(&data_dir)?;
            }

            if need_save {
                cfg.save(&config_path);
            }

            let log_dir = data_dir.join("log");
            setup_crash_handler(&log_dir);

            if let Some(crash_file) = check_last_crash(&log_dir) {
                let log_path = log_dir.to_string_lossy().to_string();
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let _ = app_handle.emit("crash-detected", serde_json::json!({
                        "file": crash_file,
                        "log_dir": log_path,
                    }));
                });
            }

            let db = database::Database::new(&data_dir)?;
            let db_state = Arc::new(Mutex::new(db));
            app.manage(DbState(db_state.clone()));
            app.manage(ConfigPath(config_path.clone()));

            let sc_str = if cfg.shortcut.is_empty() {
                "Alt+Q".to_string()
            } else {
                cfg.shortcut.clone()
            };
            hotkey::start(app.handle().clone(), &sc_str);

            clipboard::start_monitor(app.handle().clone());
            let tray = setup_tray(app, &cfg.language)?;
            app.manage(TrayState(tray));
            start_midnight_timer(app.handle().clone(), config_path, db_state);

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let config_path = app.state::<ConfigPath>();
                let cfg = AppConfig::load(&config_path.0);
                if cfg.close_to_tray {
                    let _ = window.hide();
                    api.prevent_close();
                } else {
                    app.exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_apps,
            commands::get_entries,
            commands::delete_entry,
            commands::copy_entry_to_clipboard,
            commands::clear_app_entries,
            commands::delete_entries_by_domain,
            commands::clear_database,
            commands::get_image_base64,
            commands::get_images_base64_batch,
            commands::get_entry_counts,
            commands::get_settings,
            commands::save_settings,
            commands::open_data_dir,
            commands::export_entries,
            commands::get_language_strings,
            commands::get_available_languages,
            commands::get_source_urls,
            commands::get_storage_stats,
            commands::resolve_favicon,
            commands::toggle_entry_favorite,
            commands::toggle_app_favorite,
            commands::toggle_sensitive,
            commands::get_favorite_entries,
            commands::get_favorite_counts,
            commands::dismiss_crash,
            commands::get_crash_log_content,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| eprintln!("Application error: {}", e));
}

fn start_midnight_timer(
    app_handle: tauri::AppHandle,
    config_path: std::path::PathBuf,
    db_state: Arc<Mutex<database::Database>>,
) {
    std::thread::spawn(move || loop {
        let now = chrono::Local::now();
        let secs_today = now.num_seconds_from_midnight() as u64;
        let wait = 86400u64.saturating_sub(secs_today).max(1);

        std::thread::sleep(std::time::Duration::from_secs(wait));

        let cfg = AppConfig::load(&config_path);
        let policy = &cfg.retention_policy;
        if policy != "none" {
            if let Ok(db) = db_state.lock() {
                if let Ok(image_files) = db.apply_retention_policy(policy) {
                    let images_dir = db.images_dir();
                    for f in image_files {
                        std::fs::remove_file(images_dir.join(&f)).ok();
                    }
                }
            }
            let _ = app_handle.emit("clipboard-changed", "cleared");
        }
    });
}

fn setup_tray(app: &mut tauri::App, lang: &str) -> Result<tauri::tray::TrayIcon, Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let lang_map = commands::load_language_map(lang).unwrap_or_default();
    let show_text = lang_map.get("tray.show").cloned().unwrap_or_else(|| "显示主窗口".into());
    let quit_text = lang_map.get("tray.quit").cloned().unwrap_or_else(|| "退出".into());
    let tooltip_text = lang_map.get("app.tray_tooltip").cloned().unwrap_or_else(|| "CutBoard - 剪切板管理器".into());

    let show = MenuItem::with_id(app, "show", &show_text, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", &quit_text, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("No default window icon found")?;

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .tooltip(&tooltip_text)
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(tray)
}

#[cfg(windows)]
fn acquire_single_instance_lock() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            lp: *const std::ffi::c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> *mut std::ffi::c_void;
        fn GetLastError() -> u32;
    }

    unsafe {
        let name: Vec<u16> = "Global\\CutBoard_SingleInstance\0".encode_utf16().collect();
        let _ = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        GetLastError() != 183
    }
}

#[cfg(windows)]
fn activate_existing_instance() {
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        let title: Vec<u16> = "CutBoard\0".encode_utf16().collect();
        let hwnd = FindWindowW(None, windows::core::PCWSTR(title.as_ptr()));
        if let Ok(hwnd) = hwnd {
            if !hwnd.0.is_null() {
                if !IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                }
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}
