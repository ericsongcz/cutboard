use crate::clipboard::{self, IGNORE_NEXT};
use crate::config::AppConfig;
use crate::database::{AppInfo, ClipboardEntry, SourceInfo};
use crate::{ConfigPath, DbState};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};

const IMAGE_CACHE_MAX: usize = 50;

struct ImageLruCache {
    order: VecDeque<String>,
    map: std::collections::HashMap<String, String>,
}

impl ImageLruCache {
    fn new() -> Self {
        Self { order: VecDeque::new(), map: std::collections::HashMap::new() }
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
        if self.map.len() >= IMAGE_CACHE_MAX {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }
    fn remove(&mut self, key: &str) {
        self.map.remove(key);
        self.order.retain(|k| k != key);
    }
}

static IMAGE_B64_CACHE: std::sync::LazyLock<std::sync::Mutex<ImageLruCache>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(ImageLruCache::new()));

#[tauri::command]
pub fn get_apps(app: tauri::AppHandle) -> Result<Vec<AppInfo>, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db.get_apps().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_entries(
    app: tauri::AppHandle,
    app_id: i64,
    content_type: String,
    search: Option<String>,
    source_domain: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<Vec<ClipboardEntry>, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db.get_entries(
        app_id,
        &content_type,
        search.as_deref().unwrap_or(""),
        source_domain.as_deref().unwrap_or(""),
        page.unwrap_or(1),
        page_size.unwrap_or(20),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_entry(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(image_filename) = db.delete_entry(id).map_err(|e| e.to_string())? {
        let image_path = db.images_dir().join(&image_filename);
        std::fs::remove_file(image_path).ok();
        if let Ok(mut cache) = IMAGE_B64_CACHE.lock() { cache.remove(&image_filename); }
    }
    Ok(())
}

#[tauri::command]
pub fn delete_entries_by_domain(app: tauri::AppHandle, app_id: i64, domain: String) -> Result<(), String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let image_paths = db.delete_entries_by_domain(app_id, &domain).map_err(|e| e.to_string())?;
    let images_dir = db.images_dir();
    for filename in image_paths {
        std::fs::remove_file(images_dir.join(&filename)).ok();
    }
    let _ = app.emit("clipboard-changed", ());
    Ok(())
}

#[tauri::command]
pub fn clear_app_entries(app: tauri::AppHandle, app_id: i64) -> Result<(), String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let image_paths = db.clear_app_entries(app_id).map_err(|e| e.to_string())?;
    let images_dir = db.images_dir();
    for filename in image_paths {
        std::fs::remove_file(images_dir.join(&filename)).ok();
    }
    Ok(())
}

#[tauri::command]
pub fn clear_database(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let image_paths = db.clear_all_entries().map_err(|e| e.to_string())?;
    let images_dir = db.images_dir();
    for filename in image_paths {
        std::fs::remove_file(images_dir.join(&filename)).ok();
    }
    if let Ok(mut cache) = IMAGE_B64_CACHE.lock() { *cache = ImageLruCache::new(); }
    let _ = app.emit("clipboard-changed", ());
    Ok(())
}

#[tauri::command]
pub fn copy_entry_to_clipboard(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let entry = db.get_entry_by_id(id).map_err(|e| e.to_string())?;

    IGNORE_NEXT.store(true, Ordering::SeqCst);

    match entry.content_type.as_str() {
        "text" => {
            let text = entry.text_content.as_ref().ok_or("Text content is empty")?;
            if !clipboard::write_text_to_clipboard(text) {
                IGNORE_NEXT.store(false, Ordering::SeqCst);
                return Err("Failed to write to clipboard".into());
            }
        }
        "image" => {
            let filename = entry.image_path.as_ref().ok_or("Image path is empty")?;
            let path = db.images_dir().join(filename);
            if !clipboard::write_image_to_clipboard(&path) {
                IGNORE_NEXT.store(false, Ordering::SeqCst);
                return Err("Failed to write image to clipboard".into());
            }
        }
        _ => {
            IGNORE_NEXT.store(false, Ordering::SeqCst);
            return Err("Unknown content type".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_image_base64(app: tauri::AppHandle, image_path: String) -> Result<String, String> {
    if image_path.contains("..") || image_path.contains('/') || image_path.contains('\\') {
        return Err("Invalid image path".into());
    }

    {
        let mut cache = IMAGE_B64_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.get(&image_path) {
            return Ok(cached.clone());
        }
    }

    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let images_dir = db.images_dir();
    let full_path = images_dir.join(&image_path);
    let canonical = full_path.canonicalize().map_err(|e| e.to_string())?;
    let canonical_base = images_dir.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&canonical_base) {
        return Err("Path traversal denied".into());
    }
    let data = std::fs::read(&canonical).map_err(|e| e.to_string())?;
    let result = format!("data:image/png;base64,{}", STANDARD.encode(&data));

    {
        let mut cache = IMAGE_B64_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(image_path, result.clone());
    }

    Ok(result)
}

#[tauri::command]
pub fn get_images_base64_batch(
    app: tauri::AppHandle,
    image_paths: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let images_dir = db.images_dir();
    let canonical_base = images_dir.canonicalize().map_err(|e| e.to_string())?;

    let mut result = std::collections::HashMap::new();
    let mut cache = IMAGE_B64_CACHE.lock().unwrap_or_else(|e| e.into_inner());

    for path in &image_paths {
        if path.contains("..") || path.contains('/') || path.contains('\\') {
            continue;
        }
        if let Some(cached) = cache.get(path) {
            result.insert(path.clone(), cached.clone());
            continue;
        }
        let full_path = images_dir.join(path);
        if let Ok(canonical) = full_path.canonicalize() {
            if canonical.starts_with(&canonical_base) {
                if let Ok(data) = std::fs::read(&canonical) {
                    let b64 = format!("data:image/png;base64,{}", STANDARD.encode(&data));
                    cache.insert(path.clone(), b64.clone());
                    result.insert(path.clone(), b64);
                }
            }
        }
    }
    Ok(result)
}

#[derive(Serialize)]
pub struct EntryCounts {
    pub text_count: i64,
    pub image_count: i64,
}

#[tauri::command]
pub fn get_entry_counts(
    app: tauri::AppHandle,
    app_id: i64,
    source_domain: Option<String>,
) -> Result<EntryCounts, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let (text_count, image_count) = db
        .get_entry_counts(app_id, source_domain.as_deref().unwrap_or(""))
        .map_err(|e| e.to_string())?;
    Ok(EntryCounts { text_count, image_count })
}

#[derive(Serialize)]
pub struct StorageStats {
    pub db_size: u64,
    pub images_size: u64,
    pub images_count: u64,
}

#[tauri::command]
pub fn get_storage_stats(app: tauri::AppHandle) -> Result<StorageStats, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;

    let db_path = db.db_path();
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    let images_dir = db.images_dir();
    let mut images_size: u64 = 0;
    let mut images_count: u64 = 0;
    if images_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&images_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        images_size += meta.len();
                        images_count += 1;
                    }
                }
            }
        }
    }

    Ok(StorageStats { db_size, images_size, images_count })
}

#[tauri::command]
pub fn get_source_urls(app: tauri::AppHandle, app_id: i64) -> Result<Vec<SourceInfo>, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db.get_source_urls(app_id).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct SettingsResponse {
    pub data_path: String,
    pub auto_clear_midnight: bool,
    pub auto_start: bool,
    pub close_to_tray: bool,
    pub language: String,
    pub shortcut: String,
    pub theme: String,
    pub show_copy_toast: bool,
    pub retention_policy: String,
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<SettingsResponse, String> {
    let config_path = app.state::<ConfigPath>();
    let config = AppConfig::load(&config_path.0);
    Ok(SettingsResponse {
        data_path: config.data_path,
        auto_clear_midnight: config.auto_clear_midnight,
        auto_start: config.auto_start,
        close_to_tray: config.close_to_tray,
        language: config.language,
        shortcut: config.shortcut,
        theme: config.theme,
        show_copy_toast: config.show_copy_toast,
        retention_policy: config.retention_policy,
    })
}

#[tauri::command]
pub fn save_settings(
    app: tauri::AppHandle,
    data_path: String,
    auto_clear_midnight: bool,
    auto_start: bool,
    close_to_tray: bool,
    language: String,
    shortcut: Option<String>,
    theme: Option<String>,
    show_copy_toast: Option<bool>,
    retention_policy: Option<String>,
) -> Result<(), String> {
    let config_path = app.state::<ConfigPath>();
    let old_config = AppConfig::load(&config_path.0);

    let data_dir = std::path::PathBuf::from(&data_path);
    std::fs::create_dir_all(&data_dir).map_err(|e| format!("Invalid data path: {}", e))?;

    let new_shortcut = shortcut.unwrap_or(old_config.shortcut.clone());
    let config = AppConfig {
        data_path,
        auto_clear_midnight,
        auto_start,
        close_to_tray,
        language,
        shortcut: new_shortcut.clone(),
        theme: theme.unwrap_or(old_config.theme.clone()),
        show_copy_toast: show_copy_toast.unwrap_or(old_config.show_copy_toast),
        retention_policy: retention_policy.unwrap_or(old_config.retention_policy.clone()),
    };
    config.save(&config_path.0);

    if old_config.auto_start != auto_start {
        set_auto_start_registry(auto_start)?;
    }

    if new_shortcut != old_config.shortcut {
        crate::hotkey::update(&new_shortcut);
    }

    if config.language != old_config.language || config.show_copy_toast != old_config.show_copy_toast {
        crate::clipboard::invalidate_notification_cache();
    }

    Ok(())
}

#[tauri::command]
pub fn toggle_entry_favorite(app: tauri::AppHandle, id: i64) -> Result<bool, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db.toggle_entry_favorite(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_app_favorite(app: tauri::AppHandle, id: i64) -> Result<bool, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db.toggle_app_favorite(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_sensitive(app: tauri::AppHandle, id: i64) -> Result<bool, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db.toggle_sensitive(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_favorite_entries(
    app: tauri::AppHandle,
    content_type: String,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<Vec<ClipboardEntry>, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    db.get_favorite_entries(&content_type, page.unwrap_or(1), page_size.unwrap_or(20))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_favorite_counts(app: tauri::AppHandle) -> Result<EntryCounts, String> {
    let state = app.state::<DbState>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let (text_count, image_count) = db.get_favorite_counts().map_err(|e| e.to_string())?;
    Ok(EntryCounts { text_count, image_count })
}

#[cfg(windows)]
fn set_auto_start_registry(enabled: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe_path.to_string_lossy().to_string();

    if enabled {
        let output = std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v", "CutBoard",
                "/t", "REG_SZ",
                "/d", &exe_str,
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("Failed to set auto-start registry".into());
        }
    } else {
        std::process::Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v", "CutBoard",
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok();
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_auto_start_registry(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn open_data_dir(app: tauri::AppHandle) -> Result<(), String> {
    let config_path = app.state::<ConfigPath>();
    let config = AppConfig::load(&config_path.0);
    std::process::Command::new("explorer")
        .arg(&config.data_path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn export_entries(
    app: tauri::AppHandle,
    app_id: i64,
    content_type: String,
    app_name: String,
    save_path: String,
) -> Result<String, String> {
    let state = app.state::<DbState>();
    let (entries, images_dir) = {
        let db = state.0.lock().map_err(|e| e.to_string())?;
        let entries = db
            .get_entries(app_id, &content_type, "", "", 1, 100_000)
            .map_err(|e| e.to_string())?;
        let images_dir = db.images_dir();
        (entries, images_dir)
    };

    if entries.is_empty() {
        return Err("没有可导出的记录".into());
    }

    let out_path = std::path::PathBuf::from(&save_path);

    match content_type.as_str() {
        "image" => {
            let file = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            let total = entries.len();
            for (i, entry) in entries.iter().enumerate() {
                if let Some(image_filename) = &entry.image_path {
                    let image_full = images_dir.join(image_filename);
                    if image_full.exists() {
                        zip.start_file(image_filename.as_str(), options)
                            .map_err(|e| e.to_string())?;
                        let data = std::fs::read(&image_full).map_err(|e| e.to_string())?;
                        zip.write_all(&data).map_err(|e| e.to_string())?;
                    }
                }
                let progress = ((i + 1) as f64 / total as f64 * 100.0) as u32;
                let _ = app.emit("export-progress", progress);
            }
            zip.finish().map_err(|e| e.to_string())?;

            reveal_in_explorer(&out_path);
            Ok(out_path.to_string_lossy().to_string())
        }
        "text" => {
            let mut content = format!("# CutBoard - {} 文本记录\n\n", app_name);

            let total = entries.len();
            for (i, entry) in entries.iter().enumerate() {
                if let Some(text) = &entry.text_content {
                    content.push_str(&format!(
                        "### {}\n\n{}\n\n",
                        entry.created_at, text
                    ));
                }
                let progress = ((i + 1) as f64 / total as f64 * 100.0) as u32;
                let _ = app.emit("export-progress", progress);
            }

            std::fs::write(&out_path, content.as_bytes()).map_err(|e| e.to_string())?;

            reveal_in_explorer(&out_path);
            Ok(out_path.to_string_lossy().to_string())
        }
        _ => Err("未知内容类型".into()),
    }
}

fn reveal_in_explorer(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn();
    }
}

pub fn find_language_dir() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("language");
            if p.exists() {
                return Some(p);
            }
        }
    }
    let cwd = std::path::PathBuf::from("language");
    if cwd.exists() {
        return Some(cwd);
    }
    let parent = std::path::PathBuf::from("../language");
    if parent.exists() {
        return Some(parent);
    }
    None
}

pub fn load_language_map(lang: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let lang_dir = find_language_dir().ok_or("Language directory not found")?;
    let path = lang_dir.join(format!("{}.json", lang));
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}.json: {}", lang, e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse {}.json: {}", lang, e))
}

#[tauri::command]
pub fn get_language_strings(lang: String) -> Result<std::collections::HashMap<String, String>, String> {
    load_language_map(&lang)
}

#[derive(Serialize)]
pub struct LanguageInfo {
    pub code: String,
    pub display_name: String,
}

#[tauri::command]
pub fn resolve_favicon(domain: String) -> Result<String, String> {
    let url = format!("https://{}", domain);
    let body = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;

    // Use ASCII-only lowercase to keep byte offsets identical
    let lower = body.to_ascii_lowercase();
    for pattern in &["rel=\"icon\"", "rel=\"shortcut icon\"", "rel='icon'", "rel='shortcut icon'"] {
        if let Some(pos) = lower.find(pattern) {
            let region_start = if pos > 300 { pos - 300 } else { 0 };
            let region_end = std::cmp::min(pos + 300, body.len());
            // Ensure we don't split a multi-byte character
            let region = safe_substr(&body, region_start, region_end);

            if let Some(href) = extract_href(region) {
                if href.starts_with("http://") || href.starts_with("https://") {
                    return Ok(href);
                } else if href.starts_with("//") {
                    return Ok(format!("https:{}", href));
                } else if href.starts_with('/') {
                    return Ok(format!("https://{}{}", domain, href));
                } else {
                    return Ok(format!("https://{}/{}", domain, href));
                }
            }
        }
    }

    Err("No favicon link found".into())
}

fn safe_substr(s: &str, start: usize, end: usize) -> &str {
    let start = (start..end).find(|&i| s.is_char_boundary(i)).unwrap_or(end);
    let end = (start..=end).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(start);
    &s[start..end]
}

fn extract_href(tag_region: &str) -> Option<String> {
    let lower = tag_region.to_ascii_lowercase();
    let href_pos = lower.find("href=")?;
    let after = &tag_region[href_pos + 5..];
    let trimmed = after.trim_start();
    if trimmed.starts_with('"') {
        let content = &trimmed[1..];
        let end = content.find('"')?;
        Some(content[..end].to_string())
    } else if trimmed.starts_with('\'') {
        let content = &trimmed[1..];
        let end = content.find('\'')?;
        Some(content[..end].to_string())
    } else {
        let end = trimmed.find(|c: char| c.is_whitespace() || c == '>' || c == '/')?;
        Some(trimmed[..end].to_string())
    }
}

#[tauri::command]
pub fn get_available_languages() -> Result<Vec<LanguageInfo>, String> {
    let lang_dir = find_language_dir().ok_or("Language directory not found")?;
    let mut languages = Vec::new();
    for entry in std::fs::read_dir(&lang_dir).map_err(|e| e.to_string())? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let code = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if code.starts_with('_') || code.is_empty() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, String>>(&content) {
                let display_name = map
                    .get("_language_name")
                    .cloned()
                    .unwrap_or_else(|| code.clone());
                languages.push(LanguageInfo { code, display_name });
            }
        }
    }
    languages.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(languages)
}

#[tauri::command]
pub fn dismiss_crash(app: tauri::AppHandle) -> Result<(), String> {
    let config_path = app.state::<ConfigPath>();
    let cfg = AppConfig::load(&config_path.0);
    let data_dir = std::path::PathBuf::from(&cfg.data_path);
    let log_dir = data_dir.join("log");
    if let Ok(entries) = std::fs::read_dir(&log_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("crash_") && name_str.ends_with(".log") {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_crash_log_content(app: tauri::AppHandle, file: String) -> Result<String, String> {
    let config_path = app.state::<ConfigPath>();
    let cfg = AppConfig::load(&config_path.0);
    let data_dir = std::path::PathBuf::from(&cfg.data_path);
    let log_path = data_dir.join("log").join(&file);
    if !log_path.exists() {
        return Err("Log file not found".into());
    }
    std::fs::read_to_string(&log_path).map_err(|e| e.to_string())
}
