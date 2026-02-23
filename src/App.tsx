import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "./i18n";
import AppList from "./components/AppList";
import ContentPanel from "./components/ContentPanel";
import Settings from "./components/Settings";

export interface AppInfo {
  id: number;
  name: string;
  exe_path: string;
  icon_base64: string | null;
  entry_count: number;
  is_favorite: boolean;
}

export interface ClipboardEntry {
  id: number;
  app_id: number;
  content_type: string;
  text_content: string | null;
  image_path: string | null;
  created_at: string;
  source_url: string | null;
  is_favorite: boolean;
  is_sensitive: boolean;
  html_content: string | null;
}

type ThemeMode = "light" | "dark" | "system";

function applyThemeClass(mode: ThemeMode) {
  if (mode === "dark") {
    document.documentElement.classList.add("dark");
  } else if (mode === "light") {
    document.documentElement.classList.remove("dark");
  } else {
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    if (prefersDark) document.documentElement.classList.add("dark");
    else document.documentElement.classList.remove("dark");
  }
}

function App() {
  const { t, lang } = useTranslation();
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [selectedAppId, setSelectedAppId] = useState<number | null>(null);
  const selectedAppIdRef = useRef(selectedAppId);
  selectedAppIdRef.current = selectedAppId;
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [clearToast, setClearToast] = useState<string | null>(null);
  const clearToastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [showFavorites, setShowFavorites] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  const [sensitiveAlert, setSensitiveAlert] = useState(false);
  const [themeMode, setThemeMode] = useState<ThemeMode>("system");
  const [copyToast, setCopyToast] = useState<string | null>(null);
  const copyToastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [crashInfo, setCrashInfo] = useState<{ file: string; log_dir: string } | null>(null);

  useEffect(() => {
    const title = t("app.window_title");
    if (title && title !== "app.window_title") {
      getCurrentWindow().setTitle(title).catch(() => {});
    }
  }, [t, lang]);

  const loadApps = useCallback(async () => {
    try {
      const result = await invoke<AppInfo[]>("get_apps");
      setApps(result);
      const currentId = selectedAppIdRef.current;
      if (result.length > 0 && currentId === null) {
        setSelectedAppId(result[0].id);
      }
      if (currentId !== null && !result.find((a) => a.id === currentId)) {
        setSelectedAppId(result.length > 0 ? result[0].id : null);
      }
    } catch (e) {
      console.error("Failed to load apps:", e);
    }
  }, []);

  useEffect(() => {
    loadApps();
  }, [loadApps]);

  useEffect(() => {
    const unlisten = listen("clipboard-changed", () => {
      loadApps();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadApps]);

  const handleClearApp = async (appId: number) => {
    const app = apps.find((a) => a.id === appId);
    if (!app) return;

    try {
      await invoke("clear_app_entries", { appId });
    } catch (e) {
      console.error("Failed to clear app entries:", e);
      return;
    }

    loadApps();
    if (selectedAppIdRef.current === appId) {
      setSelectedAppId(null);
    }

    setClearToast(t("undo.cleared", { name: app.name }));
    if (clearToastTimer.current) clearTimeout(clearToastTimer.current);
    clearToastTimer.current = setTimeout(() => setClearToast(null), 3000);
  };

  // Load theme from settings (shortcut is registered in Rust backend)
  useEffect(() => {
    invoke<{ theme: string; shortcut: string; show_copy_toast: boolean }>("get_settings").then((s) => {
      const mode = (s.theme || "system") as ThemeMode;
      setThemeMode(mode);
      applyThemeClass(mode);
    }).catch(() => {});
  }, []);

  // Copy toast listener
  useEffect(() => {
    const unlisten = listen<string>("copy-toast", (e) => {
      const contentType = e.payload;
      const label = contentType === "image" ? t("tabs.image") : t("tabs.text");
      const tpl = t("toast.recorded");
      setCopyToast(tpl.replace("{type}", label));
      if (copyToastTimer.current) clearTimeout(copyToastTimer.current);
      copyToastTimer.current = setTimeout(() => setCopyToast(null), 3000);
    });
    return () => {
      unlisten.then((fn) => fn());
      if (copyToastTimer.current) clearTimeout(copyToastTimer.current);
    };
  }, [t]);

  // Listen for system theme changes
  useEffect(() => {
    if (themeMode !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => applyThemeClass("system");
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [themeMode]);


  // Sensitive alert listener
  useEffect(() => {
    const unlisten = listen("sensitive-detected", () => {
      setSensitiveAlert(true);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Crash detection listener
  useEffect(() => {
    const unlisten = listen<{ file: string; log_dir: string }>("crash-detected", (e) => {
      setCrashInfo(e.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "F5" || e.key === "F12") {
        e.preventDefault();
        return;
      }
      if (e.ctrlKey) {
        const key = e.key.toLowerCase();
        if (["r", "p", "s", "u", "g"].includes(key)) {
          e.preventDefault();
          return;
        }
        if (e.shiftKey && ["i", "j", "r"].includes(key)) {
          e.preventDefault();
          return;
        }
      }
    };
    const handleContextMenu = (e: MouseEvent) => e.preventDefault();

    document.addEventListener("keydown", handleKeyDown, true);
    document.addEventListener("contextmenu", handleContextMenu, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
      document.removeEventListener("contextmenu", handleContextMenu, true);
    };
  }, []);

  return (
    <div className="h-full flex bg-gray-50 dark:bg-gray-900 text-gray-800 dark:text-gray-200 select-none">
      <AppList
        apps={apps}
        selectedAppId={selectedAppId}
        showFavorites={showFavorites}
        onSelect={(id) => { setShowFavorites(false); setSelectedAppId(id); }}
        onClear={handleClearApp}
        onOpenSettings={() => setSettingsOpen(true)}
        onToggleFavorites={() => { setShowFavorites(!showFavorites); setSelectedAppId(null); }}
        onAppFavToggle={() => { loadApps(); setRefreshKey((k) => k + 1); }}
      />
      <div className="flex-1 min-w-0">
        {showFavorites ? (
          <ContentPanel
            appId={-1}
            appName="Favorites"
            onEntryChange={loadApps}
            favoritesMode
            refreshKey={refreshKey}
          />
        ) : selectedAppId ? (
          <ContentPanel
            appId={selectedAppId}
            appName={apps.find((a) => a.id === selectedAppId)?.name ?? ""}
            onEntryChange={loadApps}
            refreshKey={refreshKey}
          />
        ) : (
          <div className="h-full flex flex-col items-center justify-center text-gray-400 gap-3">
            <svg className="w-16 h-16" viewBox="0 0 512 512" fill="none" xmlns="http://www.w3.org/2000/svg">
              <rect x="0" y="0" width="512" height="512" rx="96" ry="96" fill="#EEF2FF"/>
              <rect x="136" y="140" width="240" height="280" rx="28" ry="28" stroke="#5B8DEF" strokeWidth="24" strokeLinejoin="round" fill="none"/>
              <rect x="196" y="92" width="120" height="80" rx="16" ry="16" stroke="#5B8DEF" strokeWidth="24" strokeLinejoin="round" fill="#EEF2FF"/>
            </svg>
            <p className="text-lg">{t("content.empty_title")}</p>
            <p className="text-sm">{t("content.empty_hint")}</p>
          </div>
        )}
      </div>
      <Settings
        isOpen={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onThemeChange={(mode) => { setThemeMode(mode); applyThemeClass(mode); }}
      />

      {clearToast && (
        <div className="fixed bottom-5 left-1/2 -translate-x-1/2 flex items-center gap-2.5 px-4 py-2.5 bg-gray-800/90 dark:bg-gray-700/95 backdrop-blur text-white text-sm rounded-xl shadow-lg z-50 animate-[slideIn_0.3s_ease-out]">
          <svg className="w-4 h-4 text-green-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
          </svg>
          <span>{clearToast}</span>
        </div>
      )}


      {sensitiveAlert && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50">
          <div className="bg-white dark:bg-gray-800 rounded-xl shadow-2xl w-[380px] overflow-hidden">
            <div className="p-5">
              <div className="flex items-center gap-3 mb-3">
                <div className="w-10 h-10 rounded-full bg-amber-100 dark:bg-amber-900 flex items-center justify-center shrink-0">
                  <svg className="w-5 h-5 text-amber-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
                  </svg>
                </div>
                <h3 className="text-base font-semibold text-gray-800 dark:text-gray-200">{t("sensitive.alert_title")}</h3>
              </div>
              <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">{t("sensitive.alert_msg")}</p>
            </div>
            <div className="flex border-t border-gray-200 dark:border-gray-700">
              <button
                className="flex-1 py-3 text-sm font-medium text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/30 transition-colors"
                onClick={() => setSensitiveAlert(false)}
              >
                {t("sensitive.ok")}
              </button>
            </div>
          </div>
        </div>
      )}

      {copyToast && (
        <div className="fixed top-4 right-4 z-[70] animate-[slideIn_0.3s_ease-out] pointer-events-none">
          <div className="flex items-center gap-2.5 px-4 py-2.5 bg-gray-800/90 dark:bg-gray-700/95 backdrop-blur text-white text-sm rounded-xl shadow-lg">
            <svg className="w-4 h-4 text-green-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
            <span>{copyToast}</span>
          </div>
        </div>
      )}

      {crashInfo && (
        <div className="fixed top-0 left-0 right-0 z-[80] bg-amber-50 dark:bg-amber-900/40 border-b border-amber-200 dark:border-amber-700 px-4 py-2.5 flex items-center gap-3">
          <svg className="w-5 h-5 text-amber-500 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <span className="text-sm text-amber-800 dark:text-amber-200 flex-1">
            {t("crash.banner")}
            <span className="ml-1 text-xs text-amber-600 dark:text-amber-400 opacity-80">{crashInfo.log_dir}</span>
          </span>
          <button
            className="px-2.5 py-1 text-xs font-medium text-amber-700 dark:text-amber-300 bg-amber-200/60 dark:bg-amber-800/50 hover:bg-amber-200 dark:hover:bg-amber-800 rounded-md transition-colors"
            onClick={() => {
              invoke("dismiss_crash").catch(() => {});
              setCrashInfo(null);
            }}
          >
            {t("crash.dismiss")}
          </button>
        </div>
      )}
    </div>
  );
}

export default App;
