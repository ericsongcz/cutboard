use base64::{engine::general_purpose::STANDARD, Engine};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

const MAX_ICON_CACHE_SIZE: usize = 200;

struct LruIconCache {
    map: HashMap<String, String>,
    order: VecDeque<String>,
}

impl LruIconCache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &str) -> Option<&String> {
        if self.map.contains_key(key) {
            self.order.retain(|k| k != key);
            self.order.push_back(key.to_string());
            self.map.get(key)
        } else {
            None
        }
    }

    fn insert(&mut self, key: String, value: String) {
        if self.map.contains_key(&key) {
            self.order.retain(|k| k != &key);
            self.order.push_back(key.clone());
            self.map.insert(key, value);
            return;
        }
        if self.order.len() >= MAX_ICON_CACHE_SIZE {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }
}

static ICON_CACHE: std::sync::LazyLock<Mutex<LruIconCache>> =
    std::sync::LazyLock::new(|| Mutex::new(LruIconCache::new()));

pub struct AppWindowInfo {
    pub name: String,
    pub exe_path: String,
    pub icon_base64: Option<String>,
    pub is_self: bool,
}

#[cfg(windows)]
pub fn get_foreground_app() -> Option<AppWindowInfo> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let is_self = pid == GetCurrentProcessId();

        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);

        result.ok()?;

        let exe_path = String::from_utf16_lossy(&buf[..size as usize]);
        let name = std::path::Path::new(&exe_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        if name.is_empty() {
            return None;
        }

        let icon_base64 = get_cached_icon(&exe_path);

        Some(AppWindowInfo {
            name,
            exe_path,
            icon_base64,
            is_self,
        })
    }
}

#[cfg(not(windows))]
pub fn get_foreground_app() -> Option<AppWindowInfo> {
    None
}

#[cfg(windows)]
fn get_cached_icon(exe_path: &str) -> Option<String> {
    {
        let mut cache = ICON_CACHE.lock().ok()?;
        if let Some(icon) = cache.get(exe_path) {
            return Some(icon.clone());
        }
    }

    let icon = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extract_icon(exe_path)))
        .unwrap_or(None);
    if let Some(ref icon_data) = icon {
        if let Ok(mut cache) = ICON_CACHE.lock() {
            cache.insert(exe_path.to_string(), icon_data.clone());
        }
    }
    icon
}

#[cfg(windows)]
unsafe fn cleanup_icon_info(
    icon_info: &windows::Win32::UI::WindowsAndMessaging::ICONINFO,
    hicon: windows::Win32::UI::WindowsAndMessaging::HICON,
) {
    use windows::Win32::Graphics::Gdi::DeleteObject;
    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;
    if !icon_info.hbmColor.is_invalid() {
        let _ = DeleteObject(icon_info.hbmColor.into());
    }
    if !icon_info.hbmMask.is_invalid() {
        let _ = DeleteObject(icon_info.hbmMask.into());
    }
    let _ = DestroyIcon(hicon);
}

#[cfg(windows)]
fn extract_icon(exe_path: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, GetDC, GetDIBits, ReleaseDC, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
    use windows::Win32::UI::WindowsAndMessaging::GetIconInfo;

    unsafe {
        let path_wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut info = SHFILEINFOW::default();
        let result = SHGetFileInfoW(
            PCWSTR(path_wide.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );

        if result == 0 {
            return None;
        }

        let hicon = info.hIcon;
        let mut icon_info = std::mem::zeroed();
        if GetIconInfo(hicon, &mut icon_info).is_err() {
            cleanup_icon_info(&icon_info, hicon);
            return None;
        }

        let hbm_color = icon_info.hbmColor;
        if hbm_color.is_invalid() {
            cleanup_icon_info(&icon_info, hicon);
            return None;
        }

        let mut bm = std::mem::zeroed::<windows::Win32::Graphics::Gdi::BITMAP>();
        windows::Win32::Graphics::Gdi::GetObjectW(
            hbm_color.into(),
            std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut _),
        );

        let width = bm.bmWidth as u32;
        let height = bm.bmHeight as u32;
        if width == 0 || height == 0 {
            cleanup_icon_info(&icon_info, hicon);
            return None;
        }

        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            cleanup_icon_info(&icon_info, hicon);
            return None;
        }
        let hdc = CreateCompatibleDC(Some(hdc_screen));
        if hdc.is_invalid() {
            ReleaseDC(None, hdc_screen);
            cleanup_icon_info(&icon_info, hicon);
            return None;
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..std::mem::zeroed()
            },
            ..std::mem::zeroed()
        };

        let mut pixels = vec![0u8; (width * height * 4) as usize];
        GetDIBits(
            hdc,
            hbm_color,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = DeleteDC(hdc);
        ReleaseDC(None, hdc_screen);
        cleanup_icon_info(&icon_info, hicon);

        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }

        let img = image::RgbaImage::from_raw(width, height, pixels)?;
        let mut buf = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut buf),
            image::ImageFormat::Png,
        )
        .ok()?;

        Some(STANDARD.encode(&buf))
    }
}
