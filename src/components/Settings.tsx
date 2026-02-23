import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";

interface Props {
  isOpen: boolean;
  onClose: () => void;
  onThemeChange: (mode: "light" | "dark" | "system") => void;
}

interface SettingsData {
  data_path: string;
  auto_clear_midnight: boolean;
  auto_start: boolean;
  close_to_tray: boolean;
  language: string;
  shortcut: string;
  theme: string;
  show_copy_toast: boolean;
  retention_policy: string;
}

interface StorageStats {
  db_size: number;
  images_size: number;
  images_count: number;
}

interface LangOption {
  code: string;
  display_name: string;
}

type TabType = "general" | "data" | "privacy" | "about";
type ThemeMode = "light" | "dark" | "system";

const RETENTION_DAYS = [
  { value: "1d", label: "retention.1d" },
  { value: "3d", label: "retention.3d" },
  { value: "7d", label: "retention.7d" },
  { value: "30d", label: "retention.30d" },
];

const RETENTION_COUNT = [
  { value: "500", label: "retention.500" },
  { value: "1000", label: "retention.1000" },
  { value: "5000", label: "retention.5000" },
];

type RetentionTab = "none" | "days" | "count" | "midnight";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export default function Settings({ isOpen, onClose, onThemeChange }: Props) {
  const { t, setLang } = useTranslation();
  const [tab, setTab] = useState<TabType>("general");
  const [dataPath, setDataPath] = useState("");
  const [autoStart, setAutoStart] = useState(false);
  const [closeToTray, setCloseToTray] = useState(true);
  const [language, setLanguage] = useState("zh-CN");
  const [languages, setLanguages] = useState<LangOption[]>([]);
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [clearDone, setClearDone] = useState(false);
  const [stats, setStats] = useState<StorageStats | null>(null);
  const [shortcut, setShortcut] = useState("Alt+Q");
  const [recordingShortcut, setRecordingShortcut] = useState(false);
  const [theme, setTheme] = useState<ThemeMode>("system");
  const [showCopyToast, setShowCopyToast] = useState(true);
  const [retentionPolicy, setRetentionPolicy] = useState("none");
  const [retentionTab, setRetentionTab] = useState<RetentionTab>("days");
  const shortcutInputRef = useRef<HTMLButtonElement>(null);

  // Ref to hold latest state for the save function
  const stateRef = useRef({
    dataPath, autoStart, closeToTray, language, shortcut,
    theme, showCopyToast, retentionPolicy,
  });
  stateRef.current = {
    dataPath, autoStart, closeToTray, language, shortcut,
    theme, showCopyToast, retentionPolicy,
  };

  const persistSettings = useCallback(async (overrides: Partial<{
    dataPath: string; autoStart: boolean; closeToTray: boolean;
    language: string; shortcut: string; theme: string;
    showCopyToast: boolean; retentionPolicy: string;
  }>) => {
    const s = { ...stateRef.current, ...overrides };
    try {
      await invoke("save_settings", {
        dataPath: s.dataPath,
        autoClearMidnight: s.retentionPolicy === "midnight",
        autoStart: s.autoStart,
        closeToTray: s.closeToTray,
        language: s.language,
        shortcut: s.shortcut,
        theme: s.theme,
        showCopyToast: s.showCopyToast,
        retentionPolicy: s.retentionPolicy,
      });
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      invoke<SettingsData>("get_settings").then((s) => {
        setDataPath(s.data_path);
        setAutoStart(s.auto_start);
        setCloseToTray(s.close_to_tray);
        setLanguage(s.language);
        setLang(s.language);
        const savedTheme = (s.theme || "system") as ThemeMode;
        setTheme(savedTheme);
        onThemeChange(savedTheme);
        setShortcut(s.shortcut || "Alt+Q");
        setShowCopyToast(s.show_copy_toast !== false);
        const rp = s.retention_policy || "none";
        setRetentionPolicy(rp);
        if (rp === "none") setRetentionTab("none");
        else if (rp === "midnight") setRetentionTab("midnight");
        else if (["500", "1000", "5000"].includes(rp)) setRetentionTab("count");
        else setRetentionTab("days");
      });
      invoke<LangOption[]>("get_available_languages").then(setLanguages).catch(() => {});
      invoke<StorageStats>("get_storage_stats").then(setStats).catch(() => {});
    }
  }, [isOpen]);

  const handleClearDatabase = async () => {
    try {
      await invoke("clear_database");
      setShowClearConfirm(false);
      setClearDone(true);
      setTimeout(() => setClearDone(false), 2000);
      invoke<StorageStats>("get_storage_stats").then(setStats).catch(() => {});
    } catch (e) {
      console.error("Failed to clear database:", e);
    }
  };

  const handleOpenDir = async () => {
    try {
      await invoke("open_data_dir");
    } catch (e) {
      console.error("Failed to open data dir:", e);
    }
  };

  const handleLanguageChange = (code: string) => {
    setLanguage(code);
    setLang(code);
    persistSettings({ language: code });
  };

  const handleThemeChange = (mode: ThemeMode) => {
    setTheme(mode);
    onThemeChange(mode);
    persistSettings({ theme: mode });
  };

  const handleShortcutRecord = (e: React.KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      setRecordingShortcut(false);
      return;
    }
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");
    const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
    if (!["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
      parts.push(key);
    }
    if (parts.length >= 2 && !["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
      const newShortcut = parts.join("+");
      setShortcut(newShortcut);
      setRecordingShortcut(false);
      persistSettings({ shortcut: newShortcut });
    }
  };

  if (!isOpen) return null;

  const tabs: { key: TabType; label: string }[] = [
    { key: "general", label: t("settings.tab_general") },
    { key: "data", label: t("settings.tab_data") },
    { key: "privacy", label: t("settings.tab_privacy") },
    { key: "about", label: t("settings.tab_about") },
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        className="bg-white dark:bg-gray-800 rounded-xl shadow-2xl w-[520px] max-h-[80vh] flex flex-col overflow-hidden"
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-base font-semibold text-gray-800 dark:text-gray-200">{t("settings.title")}</h2>
          <button
            className="p-1 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
            onClick={onClose}
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="flex border-b border-gray-200 dark:border-gray-700 px-5">
          {tabs.map((item) => (
            <button
              key={item.key}
              className={`px-4 py-2.5 text-sm font-medium border-b-2 transition-colors ${
                tab === item.key
                  ? "border-blue-500 text-blue-600"
                  : "border-transparent text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
              }`}
              onClick={() => setTab(item.key)}
            >
              {item.label}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto p-5">
          {tab === "general" && (
            <div className="space-y-5">
              {/* Theme */}
              <div className="flex items-center justify-between py-3 px-4 bg-gray-50 dark:bg-gray-700 rounded-lg">
                <div>
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("settings.theme")}</div>
                  <div className="text-xs text-gray-400 mt-0.5">{t("settings.theme_hint")}</div>
                </div>
                <div className="flex items-center gap-1 bg-gray-200 dark:bg-gray-600 rounded-lg p-0.5">
                  {(["light", "system", "dark"] as ThemeMode[]).map((m) => (
                    <button
                      key={m}
                      className={`px-3 py-1 text-xs rounded-md transition-colors ${
                        theme === m
                          ? "bg-white dark:bg-gray-800 text-blue-600 shadow-sm font-medium"
                          : "text-gray-500 dark:text-gray-400 hover:text-gray-700"
                      }`}
                      onClick={() => handleThemeChange(m)}
                    >
                      {t(`settings.theme_${m}`)}
                    </button>
                  ))}
                </div>
              </div>

              {/* Language */}
              <div className="flex items-center justify-between py-3 px-4 bg-gray-50 dark:bg-gray-700 rounded-lg">
                <div>
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("settings.language")}</div>
                  <div className="text-xs text-gray-400 mt-0.5">{t("settings.language_hint")}</div>
                </div>
                <select
                  value={language}
                  onChange={(e) => handleLanguageChange(e.target.value)}
                  className="text-sm border border-gray-300 dark:border-gray-600 rounded-lg px-3 py-1.5 bg-white dark:bg-gray-800 dark:text-gray-200 focus:border-blue-400 focus:outline-none cursor-pointer"
                >
                  {languages.map((l) => (
                    <option key={l.code} value={l.code}>{l.display_name}</option>
                  ))}
                </select>
              </div>

              {/* Shortcut */}
              <div className="flex items-center justify-between py-3 px-4 bg-gray-50 dark:bg-gray-700 rounded-lg">
                <div>
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("settings.shortcut")}</div>
                  <div className="text-xs text-gray-400 mt-0.5">{t("settings.shortcut_hint")}</div>
                </div>
                <button
                  ref={shortcutInputRef}
                  className={`px-3 py-1.5 text-sm border rounded-lg transition-colors ${
                    recordingShortcut
                      ? "border-blue-500 bg-blue-50 dark:bg-blue-900/30 text-blue-600 animate-pulse"
                      : "border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-200 hover:border-blue-400"
                  }`}
                  onClick={() => setRecordingShortcut(true)}
                  onKeyDown={recordingShortcut ? handleShortcutRecord : undefined}
                  onBlur={() => setRecordingShortcut(false)}
                >
                  {recordingShortcut ? t("settings.shortcut_recording") : shortcut}
                </button>
              </div>

              {/* Auto Start */}
              <div className="flex items-center justify-between py-3 px-4 bg-gray-50 dark:bg-gray-700 rounded-lg">
                <div>
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("settings.auto_start")}</div>
                  <div className="text-xs text-gray-400 mt-0.5">{t("settings.auto_start_hint")}</div>
                </div>
                <button
                  className={`relative w-11 h-6 rounded-full transition-colors ${autoStart ? "bg-blue-500" : "bg-gray-300 dark:bg-gray-600"}`}
                  onClick={() => { const v = !autoStart; setAutoStart(v); persistSettings({ autoStart: v }); }}
                >
                  <span className={`absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform ${autoStart ? "translate-x-5" : "translate-x-0"}`} />
                </button>
              </div>

              {/* Close Behavior */}
              <div className="flex items-center justify-between py-3 px-4 bg-gray-50 dark:bg-gray-700 rounded-lg">
                <div>
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("settings.close_to_tray")}</div>
                  <div className="text-xs text-gray-400 mt-0.5">{t("settings.close_to_tray_hint")}</div>
                </div>
                <button
                  className={`relative w-11 h-6 rounded-full transition-colors ${closeToTray ? "bg-blue-500" : "bg-gray-300 dark:bg-gray-600"}`}
                  onClick={() => { const v = !closeToTray; setCloseToTray(v); persistSettings({ closeToTray: v }); }}
                >
                  <span className={`absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform ${closeToTray ? "translate-x-5" : "translate-x-0"}`} />
                </button>
              </div>

              {/* Copy Toast */}
              <div className="flex items-center justify-between py-3 px-4 bg-gray-50 dark:bg-gray-700 rounded-lg">
                <div>
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("settings.copy_toast")}</div>
                  <div className="text-xs text-gray-400 mt-0.5">{t("settings.copy_toast_hint")}</div>
                </div>
                <button
                  className={`relative w-11 h-6 rounded-full transition-colors ${showCopyToast ? "bg-blue-500" : "bg-gray-300 dark:bg-gray-600"}`}
                  onClick={() => { const v = !showCopyToast; setShowCopyToast(v); persistSettings({ showCopyToast: v }); }}
                >
                  <span className={`absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform ${showCopyToast ? "translate-x-5" : "translate-x-0"}`} />
                </button>
              </div>

              {/* Retention Policy */}
              <div className="py-3 px-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3">
                <div>
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("settings.retention")}</div>
                  <div className="text-xs text-gray-400 mt-0.5">{t("settings.retention_hint")}</div>
                </div>

                <div className="flex items-center gap-1 bg-gray-200 dark:bg-gray-600 rounded-lg p-0.5 w-fit">
                  {(["none", "days", "count", "midnight"] as RetentionTab[]).map((tb) => (
                    <button
                      key={tb}
                      className={`px-3 py-1 text-xs rounded-md transition-colors whitespace-nowrap ${
                        retentionTab === tb
                          ? "bg-white dark:bg-gray-800 text-blue-600 shadow-sm font-medium"
                          : "text-gray-500 dark:text-gray-400 hover:text-gray-700"
                      }`}
                      onClick={() => {
                        setRetentionTab(tb);
                        if (tb === "none") { setRetentionPolicy("none"); persistSettings({ retentionPolicy: "none" }); }
                        else if (tb === "midnight") { setRetentionPolicy("midnight"); persistSettings({ retentionPolicy: "midnight" }); }
                        else if (tb === "days") { const v = RETENTION_DAYS[0].value; setRetentionPolicy(v); persistSettings({ retentionPolicy: v }); }
                        else if (tb === "count") { const v = RETENTION_COUNT[0].value; setRetentionPolicy(v); persistSettings({ retentionPolicy: v }); }
                      }}
                    >
                      {t(`retention.tab_${tb}`)}
                    </button>
                  ))}
                </div>

                {(retentionTab === "days" || retentionTab === "count") && (
                  <div className="flex items-center gap-2 flex-wrap">
                    {(retentionTab === "days" ? RETENTION_DAYS : RETENTION_COUNT).map((opt) => (
                      <button
                        key={opt.value}
                        className={`px-3 py-1.5 text-xs rounded-lg border transition-colors ${
                          retentionPolicy === opt.value
                            ? "bg-blue-500 text-white border-blue-500"
                            : "bg-white dark:bg-gray-800 text-gray-600 dark:text-gray-300 border-gray-200 dark:border-gray-600 hover:border-blue-400"
                        }`}
                        onClick={() => { setRetentionPolicy(opt.value); persistSettings({ retentionPolicy: opt.value }); }}
                      >
                        {t(opt.label)}
                      </button>
                    ))}
                  </div>
                )}

                {retentionTab === "midnight" && (
                  <div className="text-xs text-gray-500 dark:text-gray-400 pl-1">
                    {t("retention.midnight")}
                  </div>
                )}
              </div>
            </div>
          )}

          {tab === "data" && (
            <div className="space-y-5">
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-200 mb-2">
                  {t("settings.data_path")}
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={dataPath}
                    onChange={(e) => setDataPath(e.target.value)}
                    onBlur={() => persistSettings({ dataPath })}
                    onKeyDown={(e) => { if (e.key === "Enter") persistSettings({ dataPath }); }}
                    className="flex-1 px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg focus:border-blue-400 focus:outline-none bg-gray-50 dark:bg-gray-700 dark:text-gray-200"
                  />
                  <button
                    className="px-3 py-2 text-sm bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-200 rounded-lg transition-colors whitespace-nowrap"
                    onClick={handleOpenDir}
                    title={t("settings.open_dir_tooltip")}
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                    </svg>
                  </button>
                </div>
                <p className="mt-1.5 text-xs text-gray-400">{t("settings.data_path_hint")}</p>
              </div>

              {stats && (
                <div className="bg-gray-50 dark:bg-gray-700 rounded-lg p-4">
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200 mb-3">{t("settings.storage_stats")}</div>
                  <div className="grid grid-cols-2 gap-3">
                    <div className="bg-white dark:bg-gray-800 rounded-lg px-4 py-3 border border-gray-100 dark:border-gray-600">
                      <div className="flex items-center gap-2 mb-1">
                        <svg className="w-4 h-4 text-blue-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4" />
                        </svg>
                        <span className="text-xs text-gray-500">{t("settings.db_size")}</span>
                      </div>
                      <div className="text-lg font-semibold text-gray-800 dark:text-gray-200">{formatSize(stats.db_size)}</div>
                    </div>
                    <div className="bg-white dark:bg-gray-800 rounded-lg px-4 py-3 border border-gray-100 dark:border-gray-600">
                      <div className="flex items-center gap-2 mb-1">
                        <svg className="w-4 h-4 text-green-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
                        </svg>
                        <span className="text-xs text-gray-500">{t("settings.images_cache")}</span>
                      </div>
                      <div className="text-lg font-semibold text-gray-800 dark:text-gray-200">{formatSize(stats.images_size)}</div>
                      <div className="text-[11px] text-gray-400 mt-0.5">{t("settings.images_count", { count: String(stats.images_count) })}</div>
                    </div>
                  </div>
                </div>
              )}

              <div className="flex items-center justify-between py-3 px-4 bg-red-50 dark:bg-red-900/20 rounded-lg border border-red-100 dark:border-red-800">
                <div>
                  <div className="text-sm font-medium text-red-700 dark:text-red-400">{t("settings.clear_db")}</div>
                  <div className="text-xs text-red-400 mt-0.5">{t("settings.clear_db_hint")}</div>
                </div>
                <div className="flex items-center gap-2">
                  {clearDone && (
                    <span className="text-xs text-green-600 flex items-center gap-1">
                      <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                      </svg>
                      {t("settings.clear_db_done")}
                    </span>
                  )}
                  <button
                    className="px-3 py-1.5 text-sm font-medium text-white bg-red-500 hover:bg-red-600 rounded-lg transition-colors"
                    onClick={() => setShowClearConfirm(true)}
                  >
                    {t("settings.clear_db")}
                  </button>
                </div>
              </div>
            </div>
          )}

          {tab === "privacy" && (
            <div className="space-y-4">
              <div className="flex items-center gap-2 mb-1">
                <svg className="w-5 h-5 text-gray-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                </svg>
                <span className="text-sm font-semibold text-gray-700 dark:text-gray-200">{t("privacy.title")}</span>
              </div>
              <div className="space-y-3">
                <div className="bg-gray-50 dark:bg-gray-700 rounded-lg px-4 py-3">
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("privacy.local_storage")}</div>
                  <div className="text-xs text-gray-400 mt-0.5 leading-relaxed">{t("privacy.local_storage_desc")}</div>
                </div>
                <div className="bg-gray-50 dark:bg-gray-700 rounded-lg px-4 py-3">
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("privacy.network_usage")}</div>
                  <div className="text-xs text-gray-400 mt-0.5 leading-relaxed">{t("privacy.network_usage_desc")}</div>
                </div>
                <div className="bg-gray-50 dark:bg-gray-700 rounded-lg px-4 py-3">
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("privacy.no_tracking")}</div>
                  <div className="text-xs text-gray-400 mt-0.5 leading-relaxed">{t("privacy.no_tracking_desc")}</div>
                </div>
              </div>

              <div className="border-t border-gray-200 dark:border-gray-600 pt-4 mt-2">
                <div className="flex items-center gap-2 mb-3">
                  <svg className="w-5 h-5 text-amber-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
                  </svg>
                  <span className="text-sm font-semibold text-gray-700 dark:text-gray-200">{t("privacy.sensitive_title")}</span>
                </div>
                <div className="space-y-3">
                  <div className="bg-gray-50 dark:bg-gray-700 rounded-lg px-4 py-3">
                    <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("privacy.sensitive_scope")}</div>
                    <div className="text-xs text-gray-400 mt-0.5 leading-relaxed">{t("privacy.sensitive_scope_desc")}</div>
                  </div>
                  <div className="bg-gray-50 dark:bg-gray-700 rounded-lg px-4 py-3">
                    <div className="text-sm font-medium text-gray-700 dark:text-gray-200">{t("privacy.sensitive_handling")}</div>
                    <div className="text-xs text-gray-400 mt-0.5 leading-relaxed">{t("privacy.sensitive_handling_desc")}</div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {tab === "about" && (
            <div className="space-y-5">
              <div className="text-center py-4">
                <div className="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-blue-50 dark:bg-blue-900/30 mb-3">
                  <svg className="w-9 h-9 text-blue-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
                  </svg>
                </div>
                <h3 className="text-lg font-semibold text-gray-800 dark:text-gray-200">CutBoard</h3>
                <p className="text-sm text-gray-500 mt-1">{t("app.subtitle")}</p>
                <p className="text-xs text-gray-400 mt-0.5">{t("about.version")}</p>
              </div>

              <div className="bg-gray-50 dark:bg-gray-700 rounded-lg p-4 text-sm text-gray-600 dark:text-gray-300 leading-relaxed">
                {t("about.description")}
              </div>
              <div className="flex items-start gap-2 mt-3 px-1">
                <svg className="w-4 h-4 text-red-500 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
                </svg>
                <p className="text-xs text-red-500 leading-relaxed">{t("about.data_warning")}</p>
              </div>
            </div>
          )}
        </div>

        {showClearConfirm && (
          <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50">
            <div className="bg-white dark:bg-gray-800 rounded-xl shadow-2xl w-[380px] overflow-hidden">
              <div className="p-5">
                <div className="flex items-center gap-3 mb-3">
                  <div className="w-10 h-10 rounded-full bg-red-100 dark:bg-red-900/30 flex items-center justify-center shrink-0">
                    <svg className="w-5 h-5 text-red-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
                    </svg>
                  </div>
                  <h3 className="text-base font-semibold text-gray-800 dark:text-gray-200">
                    {t("settings.clear_db_confirm_title")}
                  </h3>
                </div>
                <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
                  {t("settings.clear_db_confirm_msg")}
                </p>
              </div>
              <div className="flex border-t border-gray-200 dark:border-gray-700">
                <button
                  className="flex-1 py-3 text-sm font-medium text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                  onClick={() => setShowClearConfirm(false)}
                >
                  {t("settings.clear_db_cancel")}
                </button>
                <button
                  className="flex-1 py-3 text-sm font-medium text-red-600 hover:bg-red-50 dark:hover:bg-red-900/30 transition-colors border-l border-gray-200 dark:border-gray-700"
                  onClick={handleClearDatabase}
                >
                  {t("settings.clear_db_confirm")}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
