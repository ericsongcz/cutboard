use crate::{window_tracker, ConfigPath, DbState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};

fn compute_content_hash(data: &[u8]) -> String {
    // Stable FNV-1a hash (deterministic across Rust versions, unlike DefaultHasher)
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
pub static IGNORE_NEXT: AtomicBool = AtomicBool::new(false);

struct NotificationCache {
    language: String,
    show_toast: bool,
    title: String,
    text_label: String,
    image_label: String,
    body_tpl: String,
}

static NOTIFICATION_CACHE: std::sync::Mutex<Option<NotificationCache>> =
    std::sync::Mutex::new(None);

pub fn invalidate_notification_cache() {
    if let Ok(mut cache) = NOTIFICATION_CACHE.lock() {
        *cache = None;
    }
}

fn send_copy_notification(app: &AppHandle, content_type: &str) {
    let config_path = match app.try_state::<ConfigPath>() {
        Some(cp) => cp,
        None => return,
    };
    let cfg = crate::config::AppConfig::load(&config_path.0);
    if !cfg.show_copy_toast {
        return;
    }

    let _ = app.emit("copy-toast", content_type);

    let (title, body) = {
        let mut guard = NOTIFICATION_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let needs_refresh = match &*guard {
            Some(c) => c.language != cfg.language || c.show_toast != cfg.show_copy_toast,
            None => true,
        };
        if needs_refresh {
            let lang_map = crate::commands::load_language_map(&cfg.language).unwrap_or_default();
            *guard = Some(NotificationCache {
                language: cfg.language.clone(),
                show_toast: cfg.show_copy_toast,
                title: lang_map.get("app.window_title").cloned().unwrap_or_else(|| "CutBoard".into()),
                text_label: lang_map.get("tabs.text").cloned().unwrap_or_else(|| "Text".into()),
                image_label: lang_map.get("tabs.image").cloned().unwrap_or_else(|| "Image".into()),
                body_tpl: lang_map.get("toast.recorded").cloned().unwrap_or_else(|| "Recorded: {type}".into()),
            });
        }
        let c = guard.as_ref().unwrap();
        let type_label = if content_type == "image" { &c.image_label } else { &c.text_label };
        (c.title.clone(), c.body_tpl.replace("{type}", type_label))
    };

    #[cfg(windows)]
    show_balloon_notification(&title, &body);
}

#[cfg(windows)]
fn show_balloon_notification(title: &str, body: &str) {
    static BALLOON_ACTIVE: AtomicBool = AtomicBool::new(false);

    if BALLOON_ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }

    let title = title.to_string();
    let body = body.to_string();

    std::thread::spawn(move || {
        unsafe {
            balloon_notify_inner(&title, &body);
        }
        BALLOON_ACTIVE.store(false, Ordering::SeqCst);
    });
}

#[cfg(windows)]
unsafe fn balloon_notify_inner(title: &str, body: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NOTIFYICONDATAW, NOTIFY_ICON_DATA_FLAGS, NOTIFY_ICON_INFOTIP_FLAGS,
        NOTIFY_ICON_MESSAGE,
    };
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe extern "system" fn balloon_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    static REGISTERED: std::sync::Once = std::sync::Once::new();
    let class_name: Vec<u16> = "CutBoardBalloon\0".encode_utf16().collect();
    REGISTERED.call_once(|| {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(balloon_wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
    });

    let wnd_name: Vec<u16> = "\0".encode_utf16().collect();
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(class_name.as_ptr()),
        PCWSTR(wnd_name.as_ptr()),
        WINDOW_STYLE::default(),
        0,
        0,
        0,
        0,
        Some(HWND_MESSAGE),
        None,
        None,
        None,
    )
    .unwrap_or_default();

    if hwnd.0.is_null() {
        return;
    }

    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 29999;
    // NIF_ICON (0x02) | NIF_INFO (0x10)
    nid.uFlags = NOTIFY_ICON_DATA_FLAGS(0x02 | 0x10);
    // NIIF_INFO (0x01)
    nid.dwInfoFlags = NOTIFY_ICON_INFOTIP_FLAGS(0x01);

    if let Ok(icon) = LoadIconW(None, IDI_INFORMATION) {
        nid.hIcon = icon;
    }

    for (i, c) in title.encode_utf16().enumerate() {
        if i >= 63 {
            break;
        }
        nid.szInfoTitle[i] = c;
    }
    for (i, c) in body.encode_utf16().enumerate() {
        if i >= 255 {
            break;
        }
        nid.szInfo[i] = c;
    }

    // NIM_ADD (0x00) - add icon and show balloon
    let _ = Shell_NotifyIconW(NOTIFY_ICON_MESSAGE(0x00), &nid);

    std::thread::sleep(std::time::Duration::from_secs(5));

    // NIM_DELETE (0x02) - remove temporary icon
    let _ = Shell_NotifyIconW(NOTIFY_ICON_MESSAGE(0x02), &nid);
    let _ = DestroyWindow(hwnd);
}
static LAST_CONTENT_HASH: std::sync::LazyLock<std::sync::Mutex<String>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(String::new()));

// Foreground app info captured at WM_CLIPBOARDUPDATE time (before debounce)
static PENDING_APP_INFO: std::sync::LazyLock<
    std::sync::Mutex<Option<window_tracker::AppWindowInfo>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

const CF_TEXT: u32 = 1;
const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;

const MAX_TEXT_BYTES: usize = 5 * 1024 * 1024; // 5 MB

pub fn start_monitor(app: AppHandle) {
    APP_HANDLE.set(app).ok();

    #[cfg(windows)]
    std::thread::spawn(|| {
        run_windows_monitor();
    });
}

#[cfg(windows)]
fn run_windows_monitor() {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::AddClipboardFormatListener;
    use windows::Win32::UI::WindowsAndMessaging::*;

    const WM_CLIPBOARDUPDATE: u32 = 0x031D;
    const DEBOUNCE_TIMER_ID: usize = 1;
    const DEBOUNCE_MS: u32 = 300;

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CLIPBOARDUPDATE => {
                // Capture foreground app NOW, before the debounce delay
                if let Some(info) = window_tracker::get_foreground_app() {
                    if let Ok(mut pending) = PENDING_APP_INFO.lock() {
                        *pending = Some(info);
                    }
                }
                let _ = SetTimer(Some(hwnd), DEBOUNCE_TIMER_ID, DEBOUNCE_MS, None);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == DEBOUNCE_TIMER_ID => {
                let _ = KillTimer(Some(hwnd), DEBOUNCE_TIMER_ID);
                if std::panic::catch_unwind(on_clipboard_change).is_err() {
                    eprintln!("on_clipboard_change panicked, recovered");
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe {
        let class_name_str: Vec<u16> =
            "CutBoardClipboardListener\0".encode_utf16().collect();
        let class_name = PCWSTR(class_name_str.as_ptr());

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: class_name,
            ..std::mem::zeroed()
        };

        RegisterClassExW(&wc);

        let window_name: Vec<u16> = "CutBoardHidden\0".encode_utf16().collect();
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            PCWSTR(window_name.as_ptr()),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
        .unwrap_or(HWND::default());

        if hwnd.0.is_null() {
            eprintln!("Failed to create clipboard listener window");
            return;
        }

        let _ = AddClipboardFormatListener(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn on_clipboard_change() {
    if IGNORE_NEXT.swap(false, Ordering::SeqCst) {
        return;
    }

    let app = match APP_HANDLE.get() {
        Some(a) => a,
        None => return,
    };

    // Use the app info captured at WM_CLIPBOARDUPDATE time
    let app_info = match PENDING_APP_INFO.lock().ok().and_then(|mut p| p.take()) {
        Some(info) => info,
        None => match window_tracker::get_foreground_app() {
            Some(info) => info,
            None => return,
        },
    };

    if app_info.is_self {
        return;
    }

    #[cfg(windows)]
    {
        let mut content = read_clipboard_content();

        // Only keep source_url if it's a real HTTP/HTTPS URL
        if let Some(ref url) = content.source_url {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                content.source_url = None;
            }
        }

        // If text is a URL but no source_url from CF_HTML, use the text itself
        if content.source_url.is_none() {
            if let Some(ref t) = content.text {
                let trimmed = t.trim();
                if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
                    && !trimmed.contains('\n')
                {
                    content.source_url = Some(trimmed.to_string());
                }
            }
        }

        if let Some(ref t) = content.text {
            if !t.trim().is_empty() {
                let hash = compute_content_hash(t.as_bytes());
                {
                    let mut last = LAST_CONTENT_HASH.lock().unwrap_or_else(|e| e.into_inner());
                    if *last == hash {
                        return;
                    }
                    *last = hash.clone();
                }

                let current_lang = {
                    match app.try_state::<ConfigPath>() {
                        Some(cp) => crate::config::AppConfig::load(&cp.0).language,
                        None => "en".to_string(),
                    }
                };
                let is_sensitive = crate::sensitive::detect_sensitive(t, &current_lang);

                let db_state = app.state::<DbState>();
                let db = match db_state.0.lock() {
                    Ok(db) => db,
                    Err(e) => e.into_inner(),
                };
                let app_id = match db.get_or_create_app(
                    &app_info.name,
                    &app_info.exe_path,
                    app_info.icon_base64.as_deref(),
                ) {
                    Ok(id) => id,
                    Err(_) => return,
                };
                if db
                    .upsert_text_entry_with_html(
                        app_id,
                        t,
                        &hash,
                        content.source_url.as_deref(),
                        content.html.as_deref(),
                        is_sensitive,
                    )
                    .is_ok()
                {
                    drop(db);
                    if is_sensitive {
                        let _ = app.emit("sensitive-detected", "");
                    }
                    let _ = app.emit("clipboard-changed", "text");
                    send_copy_notification(app, "text");
                }
                return;
            }
        }

        if let Some(png_data) = content.image {
            let hash = compute_content_hash(&png_data);
            {
                let mut last = LAST_CONTENT_HASH.lock().unwrap_or_else(|e| e.into_inner());
                if *last == hash {
                    return;
                }
                *last = hash.clone();
            }

            let db_state = app.state::<DbState>();
            let db = match db_state.0.lock() {
                Ok(db) => db,
                Err(e) => e.into_inner(),
            };
            let app_id = match db.get_or_create_app(
                &app_info.name,
                &app_info.exe_path,
                app_info.icon_base64.as_deref(),
            ) {
                Ok(id) => id,
                Err(_) => return,
            };
            let filename = format!(
                "{}_{}.png",
                chrono::Local::now().format("%Y%m%d_%H%M%S_%3f"),
                &hash[..8]
            );
            let images_dir = db.images_dir();
            let image_path = images_dir.join(&filename);
            drop(db);

            if std::fs::write(&image_path, &png_data).is_ok() {
                let db = match db_state.0.lock() {
                    Ok(db) => db,
                    Err(e) => e.into_inner(),
                };
                match db.upsert_image_entry(app_id, &filename, &hash, content.source_url.as_deref())
                {
                    Ok((_id, was_duplicate)) => {
                        drop(db);
                        if was_duplicate {
                            std::fs::remove_file(&image_path).ok();
                        }
                        let _ = app.emit("clipboard-changed", "image");
                        send_copy_notification(app, "image");
                    }
                    Err(_) => {
                        drop(db);
                        std::fs::remove_file(&image_path).ok();
                    }
                }
            }
        }
    }
}

#[cfg(windows)]
struct ClipboardContent {
    text: Option<String>,
    image: Option<Vec<u8>>,
    source_url: Option<String>,
    html: Option<String>,
}

#[cfg(windows)]
unsafe fn open_clipboard_with_retry(max_retries: u32) -> bool {
    use windows::Win32::System::DataExchange::OpenClipboard;
    for i in 0..max_retries {
        if OpenClipboard(None).is_ok() {
            return true;
        }
        if i < max_retries - 1 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    false
}

#[cfg(windows)]
fn read_clipboard_content() -> ClipboardContent {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    let mut result = ClipboardContent {
        text: None,
        image: None,
        source_url: None,
        html: None,
    };

    unsafe {
        if !open_clipboard_with_retry(5) {
            return result;
        }

        // --- Read CF_HTML for SourceURL and HTML fragment ---
        let format_name: Vec<u16> = "HTML Format\0".encode_utf16().collect();
        let cf_html = RegisterClipboardFormatW(PCWSTR(format_name.as_ptr()));
        if cf_html != 0 {
            if let Ok(handle) = GetClipboardData(cf_html) {
                let hglobal = HGLOBAL(handle.0);
                let ptr = GlobalLock(hglobal) as *const u8;
                if !ptr.is_null() {
                    let size = GlobalSize(hglobal);
                    if size > 0 {
                        let data = std::slice::from_raw_parts(ptr, size);
                        let header = String::from_utf8_lossy(data);
                        let header_str = header.to_string();

                        for line in header_str.lines() {
                            if let Some(url) = line.strip_prefix("SourceURL:") {
                                let url = url.trim();
                                if !url.is_empty() {
                                    result.source_url = Some(url.to_string());
                                    break;
                                }
                            }
                        }

                        // Extract HTML fragment between StartFragment and EndFragment
                        let mut start_frag: Option<usize> = None;
                        let mut end_frag: Option<usize> = None;
                        for line in header_str.lines() {
                            if let Some(v) = line.strip_prefix("StartFragment:") {
                                start_frag = v.trim().parse().ok();
                            }
                            if let Some(v) = line.strip_prefix("EndFragment:") {
                                end_frag = v.trim().parse().ok();
                            }
                        }
                        if let (Some(s), Some(e)) = (start_frag, end_frag) {
                            if s < e && e <= data.len() {
                                let fragment = String::from_utf8_lossy(&data[s..e]).to_string();
                                if !fragment.trim().is_empty() && fragment.len() <= MAX_TEXT_BYTES {
                                    result.html = Some(fragment);
                                }
                            }
                        }
                    }
                    let _ = GlobalUnlock(hglobal);
                }
            }
        }

        // --- Read text: CF_UNICODETEXT first, then CF_TEXT fallback ---
        if let Ok(handle) = GetClipboardData(CF_UNICODETEXT) {
            let hglobal = HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal) as *const u16;
            if !ptr.is_null() {
                let size = GlobalSize(hglobal);
                let max_chars = if size >= 2 { size / 2 } else { MAX_TEXT_BYTES };
                let len = (0..max_chars).take_while(|&i| *ptr.add(i) != 0).count();
                let slice = std::slice::from_raw_parts(ptr, len);
                let text = String::from_utf16_lossy(slice);
                if text.len() <= MAX_TEXT_BYTES {
                    result.text = Some(text);
                }
                let _ = GlobalUnlock(hglobal);
            }
        }

        // Fallback: CF_TEXT (ANSI) for legacy apps
        if result.text.is_none() {
            if let Ok(handle) = GetClipboardData(CF_TEXT) {
                let hglobal = HGLOBAL(handle.0);
                let ptr = GlobalLock(hglobal) as *const u8;
                if !ptr.is_null() {
                    let size = GlobalSize(hglobal);
                    if size > 0 {
                        let data = std::slice::from_raw_parts(ptr, size);
                        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
                        let bytes = &data[..end];
                        if bytes.len() <= MAX_TEXT_BYTES {
                            // Try UTF-8 first, then Windows-1252/Latin-1
                            result.text = Some(match std::str::from_utf8(bytes) {
                                Ok(s) => s.to_string(),
                                Err(_) => bytes.iter().map(|&b| b as char).collect(),
                            });
                        }
                    }
                    let _ = GlobalUnlock(hglobal);
                }
            }
        }

        // --- Read image (only if no usable text) ---
        if result.text.as_ref().map_or(true, |t| t.trim().is_empty()) {
            result.image = try_read_clipboard_image();
        }

        let _ = CloseClipboard();
    }

    result
}

#[cfg(windows)]
unsafe fn try_read_clipboard_image() -> Option<Vec<u8>> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    // 1. Try CF_PNG (registered format "PNG") — raw PNG bytes, most reliable
    for name in &["PNG\0", "image/png\0"] {
        let fmt_name: Vec<u16> = name.encode_utf16().collect();
        let cf_png = RegisterClipboardFormatW(PCWSTR(fmt_name.as_ptr()));
        if cf_png != 0 {
            if let Ok(handle) = GetClipboardData(cf_png) {
                let hglobal = HGLOBAL(handle.0);
                let ptr = GlobalLock(hglobal) as *const u8;
                if !ptr.is_null() {
                    let size = GlobalSize(hglobal);
                    if size > 8 {
                        let data = std::slice::from_raw_parts(ptr, size);
                        // Verify PNG magic bytes
                        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                            let png_data = data.to_vec();
                            let _ = GlobalUnlock(hglobal);
                            return Some(png_data);
                        }
                    }
                    let _ = GlobalUnlock(hglobal);
                }
            }
        }
    }

    // 2. Try CF_DIBV5 (format 17) — newer DIB with alpha support
    if let Ok(handle) = GetClipboardData(CF_DIBV5) {
        let hglobal = HGLOBAL(handle.0);
        let ptr = GlobalLock(hglobal) as *const u8;
        if !ptr.is_null() {
            let size = GlobalSize(hglobal);
            if size > 0 {
                let data = std::slice::from_raw_parts(ptr, size);
                let result = dib_to_png(data);
                let _ = GlobalUnlock(hglobal);
                if result.is_some() {
                    return result;
                }
            } else {
                let _ = GlobalUnlock(hglobal);
            }
        }
    }

    // 3. Try CF_DIB (format 8) — standard DIB
    if let Ok(handle) = GetClipboardData(CF_DIB) {
        let hglobal = HGLOBAL(handle.0);
        let ptr = GlobalLock(hglobal) as *const u8;
        if !ptr.is_null() {
            let size = GlobalSize(hglobal);
            if size > 0 {
                let data = std::slice::from_raw_parts(ptr, size);
                let result = dib_to_png(data);
                let _ = GlobalUnlock(hglobal);
                return result;
            }
            let _ = GlobalUnlock(hglobal);
        }
    }

    None
}

#[cfg(windows)]
fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 40 {
        return None;
    }

    let header_size = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    let width = i32::from_le_bytes(dib[4..8].try_into().ok()?);
    let height = i32::from_le_bytes(dib[8..12].try_into().ok()?);
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);

    if width <= 0 || width > 4096 || height == 0 || height.unsigned_abs() > 4096 {
        return None;
    }

    let pixel_count = (width as u64) * (height.unsigned_abs() as u64);
    if pixel_count > 16_000_000 {
        return None;
    }

    // BI_RGB = 0, BI_BITFIELDS = 3
    if compression != 0 && compression != 3 {
        return None;
    }

    let abs_height = height.unsigned_abs() as u32;
    let w = width as u32;
    let top_down = height < 0;

    let mut pixel_offset = header_size;

    // For 8-bit images, skip the color palette
    if bit_count == 8 {
        let colors_used = u32::from_le_bytes(dib[32..36].try_into().ok()?) as usize;
        let palette_count = if colors_used == 0 { 256 } else { colors_used };
        pixel_offset = header_size + palette_count * 4;
    }
    // For BI_BITFIELDS, 3 DWORD masks follow the header
    if compression == 3 && header_size < 52 {
        pixel_offset = header_size + 12;
    }

    if pixel_offset >= dib.len() {
        return None;
    }
    let pixels_raw = &dib[pixel_offset..];

    let mut img = image::RgbaImage::new(w, abs_height);

    match bit_count {
        32 => {
            let row_bytes = (w * 4) as usize;
            for y in 0..abs_height {
                let src_y = if top_down { y } else { abs_height - 1 - y };
                let row_start = src_y as usize * row_bytes;
                if row_start + row_bytes > pixels_raw.len() {
                    break;
                }
                for x in 0..w {
                    let off = row_start + (x as usize) * 4;
                    let b = pixels_raw[off];
                    let g = pixels_raw[off + 1];
                    let r = pixels_raw[off + 2];
                    let a = pixels_raw[off + 3];
                    let alpha = if a == 0 { 255 } else { a };
                    img.put_pixel(x, y, image::Rgba([r, g, b, alpha]));
                }
            }
        }
        24 => {
            let row_bytes = ((w * 3 + 3) & !3) as usize;
            for y in 0..abs_height {
                let src_y = if top_down { y } else { abs_height - 1 - y };
                let row_start = src_y as usize * row_bytes;
                if row_start + (w as usize) * 3 > pixels_raw.len() {
                    break;
                }
                for x in 0..w {
                    let off = row_start + (x as usize) * 3;
                    let b = pixels_raw[off];
                    let g = pixels_raw[off + 1];
                    let r = pixels_raw[off + 2];
                    img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                }
            }
        }
        16 => {
            let row_bytes = ((w * 2 + 3) & !3) as usize;
            for y in 0..abs_height {
                let src_y = if top_down { y } else { abs_height - 1 - y };
                let row_start = src_y as usize * row_bytes;
                if row_start + (w as usize) * 2 > pixels_raw.len() {
                    break;
                }
                for x in 0..w {
                    let off = row_start + (x as usize) * 2;
                    let pixel16 =
                        u16::from_le_bytes([pixels_raw[off], pixels_raw[off + 1]]);
                    // Default 5-5-5 format
                    let r = ((pixel16 >> 10) & 0x1F) as u8 * 255 / 31;
                    let g = ((pixel16 >> 5) & 0x1F) as u8 * 255 / 31;
                    let b = (pixel16 & 0x1F) as u8 * 255 / 31;
                    img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                }
            }
        }
        8 => {
            // 8-bit indexed color with palette
            let colors_used =
                u32::from_le_bytes(dib[32..36].try_into().ok()?) as usize;
            let palette_count = if colors_used == 0 { 256 } else { colors_used };
            let palette_start = header_size;
            if palette_start + palette_count * 4 > dib.len() {
                return None;
            }
            let palette = &dib[palette_start..palette_start + palette_count * 4];

            let row_bytes = ((w + 3) & !3) as usize;
            for y in 0..abs_height {
                let src_y = if top_down { y } else { abs_height - 1 - y };
                let row_start = src_y as usize * row_bytes;
                if row_start + w as usize > pixels_raw.len() {
                    break;
                }
                for x in 0..w {
                    let idx = pixels_raw[row_start + x as usize] as usize;
                    if idx < palette_count {
                        let po = idx * 4;
                        let b = palette[po];
                        let g = palette[po + 1];
                        let r = palette[po + 2];
                        img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                    }
                }
            }
        }
        _ => return None,
    }

    let mut buf = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Png,
    )
    .ok()?;
    Some(buf)
}

#[cfg(windows)]
pub fn write_text_to_clipboard(text: &str) -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;

    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }

        let _ = EmptyClipboard();

        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;

        let success = match GlobalAlloc(GLOBAL_ALLOC_FLAGS(0x0002), size) {
            Ok(hmem) => {
                let ptr = GlobalLock(hmem) as *mut u16;
                if ptr.is_null() {
                    false
                } else {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                    let _ = GlobalUnlock(hmem);
                    SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hmem.0))).is_ok()
                }
            }
            Err(_) => false,
        };

        let _ = CloseClipboard();
        success
    }
}

#[cfg(windows)]
pub fn write_image_to_clipboard(png_path: &std::path::Path) -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;

    let img = match image::open(png_path) {
        Ok(img) => img.to_rgba8(),
        Err(_) => return false,
    };

    let width = img.width() as i32;
    let height = img.height() as i32;

    let row_bytes = (width as usize) * 4;
    let pixel_size = row_bytes * (height as usize);
    let header_size = 40usize;
    let total_size = header_size + pixel_size;

    let mut dib = vec![0u8; total_size];
    dib[0..4].copy_from_slice(&(header_size as u32).to_le_bytes());
    dib[4..8].copy_from_slice(&width.to_le_bytes());
    dib[8..12].copy_from_slice(&height.to_le_bytes());
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32u16.to_le_bytes());

    for y in 0..height as u32 {
        for x in 0..width as u32 {
            let pixel = img.get_pixel(x, y);
            let dst_y = (height as u32 - 1 - y) as usize;
            let off = header_size + dst_y * row_bytes + (x as usize) * 4;
            dib[off] = pixel[2];
            dib[off + 1] = pixel[1];
            dib[off + 2] = pixel[0];
            dib[off + 3] = pixel[3];
        }
    }

    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();

        let success = match GlobalAlloc(GLOBAL_ALLOC_FLAGS(0x0002), total_size) {
            Ok(hmem) => {
                let ptr = GlobalLock(hmem) as *mut u8;
                if ptr.is_null() {
                    false
                } else {
                    std::ptr::copy_nonoverlapping(dib.as_ptr(), ptr, total_size);
                    let _ = GlobalUnlock(hmem);
                    SetClipboardData(CF_DIB, Some(HANDLE(hmem.0))).is_ok()
                }
            }
            Err(_) => false,
        };

        let _ = CloseClipboard();
        success
    }
}

#[cfg(not(windows))]
pub fn write_text_to_clipboard(_text: &str) -> bool {
    false
}

#[cfg(not(windows))]
pub fn write_image_to_clipboard(_path: &std::path::Path) -> bool {
    false
}
