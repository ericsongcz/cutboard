use std::path::{Path, PathBuf};

fn detect_system_language() -> String {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultLocaleName(buf: *mut u16, len: i32) -> i32;
        }

        let mut buf = [0u16; 85];
        let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if len > 0 {
            let locale = String::from_utf16_lossy(&buf[..((len - 1) as usize)]);
            return map_locale_to_language(&locale);
        }
    }
    "en".to_string()
}

fn map_locale_to_language(locale: &str) -> String {
    let supported = [
        "zh-CN", "zh-TW", "en", "ja", "ko", "fr", "de", "es", "pt",
        "ru", "ar", "th", "vi", "it", "nl", "pl", "tr", "uk", "id", "hi",
    ];

    let normalized = locale.replace('_', "-");

    // Exact match (e.g., "zh-CN" -> "zh-CN")
    for lang in &supported {
        if normalized.eq_ignore_ascii_case(lang) {
            return lang.to_string();
        }
    }

    // zh-HK, zh-MO -> zh-TW (Traditional Chinese)
    let lower = normalized.to_lowercase();
    if lower.starts_with("zh-hk") || lower.starts_with("zh-mo") || lower.starts_with("zh-hant") {
        return "zh-TW".to_string();
    }
    if lower.starts_with("zh") {
        return "zh-CN".to_string();
    }

    // Prefix match (e.g., "en-US" -> "en", "fr-FR" -> "fr", "pt-BR" -> "pt")
    let prefix = normalized.split('-').next().unwrap_or("en").to_lowercase();
    for lang in &supported {
        if lang.to_lowercase() == prefix {
            return lang.to_string();
        }
    }

    "en".to_string()
}

#[derive(Debug, Clone)]
pub struct AppConfig {
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

impl AppConfig {
    pub fn load(config_path: &Path) -> Self {
        let content = match std::fs::read_to_string(config_path) {
            Ok(c) => c,
            Err(_) => return Self::with_default_path(""),
        };

        let mut data_path = String::new();
        let mut auto_clear = false;
        let mut auto_start = false;
        let mut close_to_tray = true;
        let mut language = detect_system_language();
        let mut shortcut = String::from("Alt+Q");
        let mut theme = String::from("system");
        let mut show_copy_toast = true;
        let mut retention_policy = String::from("none");

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "data_path" => data_path = value.trim().to_string(),
                    "auto_clear_midnight" => auto_clear = value.trim() == "true",
                    "auto_start" => auto_start = value.trim() == "true",
                    "close_to_tray" => close_to_tray = value.trim() != "false",
                    "language" => language = value.trim().to_string(),
                    "shortcut" => shortcut = value.trim().to_string(),
                    "theme" => theme = value.trim().to_string(),
                    "show_copy_toast" => show_copy_toast = value.trim() != "false",
                    "retention_policy" => retention_policy = value.trim().to_string(),
                    _ => {}
                }
            }
        }

        // Backward compat: if auto_clear_midnight=true and retention_policy not set explicitly
        if auto_clear && retention_policy == "none" {
            retention_policy = "midnight".to_string();
        }

        Self {
            data_path,
            auto_clear_midnight: auto_clear,
            auto_start,
            close_to_tray,
            language,
            shortcut,
            theme,
            show_copy_toast,
            retention_policy,
        }
    }

    pub fn save(&self, config_path: &Path) {
        let content = format!(
            "; CutBoard 配置文件\n\
             data_path={}\n\
             auto_clear_midnight={}\n\
             auto_start={}\n\
             close_to_tray={}\n\
             language={}\n\
             shortcut={}\n\
             theme={}\n\
             show_copy_toast={}\n\
             retention_policy={}\n",
            self.data_path,
            self.auto_clear_midnight,
            self.auto_start,
            self.close_to_tray,
            self.language,
            self.shortcut,
            self.theme,
            self.show_copy_toast,
            self.retention_policy,
        );
        if let Some(parent) = config_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("Failed to create config directory: {}", e);
                return;
            }
        }
        if let Err(e) = std::fs::write(config_path, content) {
            eprintln!("Failed to save config: {}", e);
        }
    }

    pub fn with_default_path(default: &str) -> Self {
        Self {
            data_path: default.to_string(),
            auto_clear_midnight: false,
            auto_start: false,
            close_to_tray: true,
            language: detect_system_language(),
            shortcut: String::from("Alt+Q"),
            theme: String::from("system"),
            show_copy_toast: true,
            retention_policy: String::from("none"),
        }
    }

    pub fn config_file_path(_app_data_dir: &Path) -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join("config.ini");
            }
        }
        _app_data_dir.join("config.ini")
    }
}
