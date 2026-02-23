import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "../i18n";
import type { ClipboardEntry } from "../App";
import TextList from "./TextList";
import ImageList from "./ImageList";
import SourceList from "./SourceList";
import type { SourceInfo } from "./SourceList";

interface Props {
  appId: number;
  appName: string;
  onEntryChange: () => void;
  favoritesMode?: boolean;
  refreshKey?: number;
}

type TabType = "text" | "image";

const PAGE_SIZE = 20;

export default function ContentPanel({ appId, appName, onEntryChange, favoritesMode, refreshKey }: Props) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<TabType>("text");
  const [entries, setEntries] = useState<ClipboardEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const debounceTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const [textCount, setTextCount] = useState(0);
  const [imageCount, setImageCount] = useState(0);
  const [currentPage, setCurrentPage] = useState(1);
  const [exporting, setExporting] = useState(false);
  const [exportProgress, setExportProgress] = useState(0);
  const [exportDone, setExportDone] = useState<string | null>(null);
  const [deleteToast, setDeleteToast] = useState(false);
  const deleteToastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const loadEntriesRef = useRef<() => Promise<void>>(null as unknown as () => Promise<void>);
  const exportUnlistenRef = useRef<(() => void) | null>(null);
  const [sources, setSources] = useState<SourceInfo[]>([]);
  const [selectedDomain, setSelectedDomain] = useState<string | null>(null);

  const totalCount = activeTab === "text" ? textCount : imageCount;
  const totalPages = Math.max(1, Math.ceil(totalCount / PAGE_SIZE));

  const handleSearchChange = (value: string) => {
    setSearchQuery(value);
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    debounceTimer.current = setTimeout(() => {
      setDebouncedSearch(value);
      setCurrentPage(1);
    }, 300);
  };

  useEffect(() => {
    return () => {
      if (debounceTimer.current) clearTimeout(debounceTimer.current);
    };
  }, []);

  const loadSources = useCallback(async () => {
    try {
      const result = await invoke<SourceInfo[]>("get_source_urls", { appId });
      setSources(result);
    } catch {
      setSources([]);
    }
  }, [appId]);

  useEffect(() => {
    setSearchQuery("");
    setDebouncedSearch("");
    setSelectedDomain(null);
    setCurrentPage(1);
    loadSources();
  }, [appId, loadSources]);

  useEffect(() => {
    setCurrentPage(1);
  }, [activeTab, selectedDomain]);

  const loadCounts = useCallback(async () => {
    try {
      if (favoritesMode) {
        const result = await invoke<{ text_count: number; image_count: number }>("get_favorite_counts");
        setTextCount(result.text_count);
        setImageCount(result.image_count);
      } else {
        const result = await invoke<{ text_count: number; image_count: number }>(
          "get_entry_counts",
          { appId, sourceDomain: selectedDomain || undefined }
        );
        setTextCount(result.text_count);
        setImageCount(result.image_count);
      }
    } catch (e) {
      console.error("Failed to load counts:", e);
    }
  }, [appId, selectedDomain, favoritesMode, refreshKey]);

  const loadEntries = useCallback(async () => {
    setLoading(true);
    try {
      let result: ClipboardEntry[];
      if (favoritesMode) {
        result = await invoke<ClipboardEntry[]>("get_favorite_entries", {
          contentType: activeTab,
          page: currentPage,
          pageSize: PAGE_SIZE,
        });
      } else {
        result = await invoke<ClipboardEntry[]>("get_entries", {
          appId,
          contentType: activeTab,
          search: debouncedSearch || undefined,
          sourceDomain: selectedDomain || undefined,
          page: currentPage,
          pageSize: PAGE_SIZE,
        });
      }
      setEntries(result);
      loadCounts();
    } catch (e) {
      console.error("Failed to load entries:", e);
    } finally {
      setLoading(false);
    }
  }, [appId, activeTab, debouncedSearch, selectedDomain, currentPage, loadCounts, favoritesMode, refreshKey]);

  loadEntriesRef.current = loadEntries;

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  useEffect(() => {
    const unlisten = listen("clipboard-changed", () => {
      loadEntries();
      loadSources();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadEntries, loadSources]);

  const handleDelete = async (id: number) => {
    setEntries((prev) => prev.filter((e) => e.id !== id));
    if (activeTab === "text") setTextCount((c) => c - 1);
    else setImageCount((c) => c - 1);

    try {
      await invoke("delete_entry", { id });
      onEntryChange();
    } catch (e) {
      console.error("Failed to delete entry:", e);
      loadEntriesRef.current();
      return;
    }

    setDeleteToast(true);
    if (deleteToastTimer.current) clearTimeout(deleteToastTimer.current);
    deleteToastTimer.current = setTimeout(() => setDeleteToast(false), 2000);
  };

  const handleCopy = async (id: number) => {
    try {
      await invoke("copy_entry_to_clipboard", { id });
    } catch (e) {
      console.error("Failed to copy entry:", e);
    }
  };

  const handleToggleFavorite = async (id: number) => {
    try {
      await invoke("toggle_entry_favorite", { id });
      loadEntries();
    } catch (e) {
      console.error("Failed to toggle favorite:", e);
    }
  };

  const handleToggleSensitive = async (id: number) => {
    try {
      await invoke("toggle_sensitive", { id });
      loadEntries();
    } catch (e) {
      console.error("Failed to toggle sensitive:", e);
    }
  };

  const handleDeleteDomain = async (domain: string) => {
    try {
      await invoke("delete_entries_by_domain", { appId, domain });
      if (selectedDomain === domain) setSelectedDomain(null);
      onEntryChange();
      loadEntries();
    } catch (e) {
      console.error("Failed to delete domain entries:", e);
    }
  };

  useEffect(() => {
    return () => {
      if (exportUnlistenRef.current) {
        exportUnlistenRef.current();
        exportUnlistenRef.current = null;
      }
    };
  }, []);

  const handleExport = async () => {
    const now = new Date();
    const dateStr = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, "0")}${String(now.getDate()).padStart(2, "0")}`;
    const defaultName = activeTab === "image"
      ? `CutBoard_${appName}_${dateStr}.zip`
      : `CutBoard_${appName}_${dateStr}.md`;

    const savePath = await save({
      defaultPath: defaultName,
      filters: activeTab === "image"
        ? [{ name: "ZIP", extensions: ["zip"] }]
        : [{ name: "Markdown", extensions: ["md"] }],
    });

    if (!savePath) return;

    setExporting(true);
    setExportProgress(0);
    setExportDone(null);

    const unlisten = await listen<number>("export-progress", (event) => {
      setExportProgress(event.payload);
    });
    exportUnlistenRef.current = unlisten;

    try {
      const path = await invoke<string>("export_entries", {
        appId,
        contentType: activeTab,
        appName,
        savePath,
      });
      setExportDone(path);
      setTimeout(() => setExportDone(null), 4000);
    } catch (e) {
      console.error("Export failed:", e);
    } finally {
      setExporting(false);
      unlisten();
      exportUnlistenRef.current = null;
    }
  };

  const tabs = useMemo<{ key: TabType; label: string; icon: React.ReactNode }[]>(() => [
    {
      key: "text",
      label: t("tabs.text"),
      icon: (
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
        </svg>
      ),
    },
    {
      key: "image",
      label: t("tabs.image"),
      icon: (
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      ),
    },
  ], [t]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const goToPage = (p: number) => {
    const clamped = Math.max(1, Math.min(p, totalPages));
    setCurrentPage(clamped);
    scrollRef.current?.scrollTo({ top: 0 });
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        setCurrentPage((p) => Math.max(1, p - 1));
        scrollRef.current?.scrollTo({ top: 0 });
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        setCurrentPage((p) => Math.min(totalPages, p + 1));
        scrollRef.current?.scrollTo({ top: 0 });
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [totalPages]);

  const getPageNumbers = (current: number, total: number): (number | "...")[] => {
    if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1);
    const pages: (number | "...")[] = [];
    pages.push(1);
    if (current > 3) pages.push("...");
    const start = Math.max(2, current - 1);
    const end = Math.min(total - 1, current + 1);
    for (let i = start; i <= end; i++) pages.push(i);
    if (current < total - 2) pages.push("...");
    pages.push(total);
    return pages;
  };

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 px-4 gap-2 shrink-0 h-12">
        <div className="flex shrink-0">
          {tabs.map((tab) => (
            <button
              key={tab.key}
              className={`flex items-center gap-1.5 px-4 h-full text-sm font-medium border-b-2 transition-colors whitespace-nowrap ${
                activeTab === tab.key
                  ? "border-blue-500 text-blue-600"
                  : "border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300"
              }`}
              onClick={() => setActiveTab(tab.key)}
            >
              {tab.icon}
              {tab.label}
              <span className="text-[11px] text-gray-400 font-normal ml-1">
                | {tab.key === "text" ? textCount : imageCount}
              </span>
            </button>
          ))}
        </div>

        <div className="ml-auto flex items-center gap-3 min-w-0">
          {activeTab === "text" && (
            <div className="relative min-w-0 flex-1 max-w-[11rem]">
              <svg
                className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-400"
                fill="none" viewBox="0 0 24 24" stroke="currentColor"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => handleSearchChange(e.target.value)}
                placeholder={t("search.placeholder")}
                className="w-full pl-8 pr-8 py-1.5 text-xs border border-gray-200 dark:border-gray-600 rounded-lg bg-gray-50 dark:bg-gray-700 focus:bg-white dark:focus:bg-gray-600 focus:border-blue-400 focus:outline-none transition-colors dark:text-gray-200"
              />
              {searchQuery && (
                <button
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600"
                  onClick={() => { setSearchQuery(""); setDebouncedSearch(""); setCurrentPage(1); }}
                >
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              )}
            </div>
          )}
          <span className="text-xs text-gray-400 whitespace-nowrap shrink-0">
            {t("content.records", { count: String(totalCount) })}
          </span>
        </div>
      </div>

      <div className="flex-1 flex min-h-0">
        {sources.length > 0 && (
          <SourceList
            sources={sources}
            selectedDomain={selectedDomain}
            onSelect={(d) => { setSelectedDomain(d); }}
            onDeleteDomain={handleDeleteDomain}
          />
        )}
      <div ref={scrollRef} className="flex-1 overflow-y-auto relative min-h-0" id="content-scroll">
        {entries.length > 0 && (
          <div className="sticky top-0 left-0 right-0 h-3 z-[1] pointer-events-none bg-gradient-to-b from-gray-50 dark:from-gray-900 to-transparent" />
        )}

        {loading ? (
          <div className="flex items-center justify-center h-32 text-gray-400">
            {t("content.loading")}
          </div>
        ) : entries.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-gray-400 gap-2">
            {debouncedSearch ? (
              <p>{t("content.no_match", { query: debouncedSearch })}</p>
            ) : (
              <p>{activeTab === "text" ? t("content.no_text") : t("content.no_image")}</p>
            )}
          </div>
        ) : activeTab === "text" ? (
          <TextList entries={entries} onDelete={handleDelete} onCopy={handleCopy} onToggleFavorite={handleToggleFavorite} onToggleSensitive={handleToggleSensitive} />
        ) : (
          <ImageList entries={entries} onDelete={handleDelete} onCopy={handleCopy} onToggleFavorite={handleToggleFavorite} />
        )}

        {entries.length > 0 && (
          <div className="sticky bottom-0 left-0 right-0 h-6 z-[1] pointer-events-none bg-gradient-to-t from-gray-50 dark:from-gray-900 to-transparent" />
        )}

        {exporting && (
          <div className="absolute inset-0 bg-white/80 backdrop-blur-sm flex flex-col items-center justify-center gap-4 z-10">
            <div className="w-64 space-y-2">
              <div className="flex items-center justify-between text-sm text-gray-600">
                <span>{t("export.progress")}</span>
                <span>{exportProgress}%</span>
              </div>
              <div className="w-full h-2 bg-gray-200 rounded-full overflow-hidden">
                <div
                  className="h-full bg-blue-500 rounded-full transition-all duration-200"
                  style={{ width: `${exportProgress}%` }}
                />
              </div>
            </div>
          </div>
        )}

        {exportDone && (
          <div className="absolute bottom-2 left-1/2 -translate-x-1/2 flex items-center gap-2 px-4 py-2 bg-green-500 text-white text-sm rounded-lg shadow-lg z-10">
            <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
            <span className="truncate max-w-xs">{t("export.done")}</span>
          </div>
        )}

        {deleteToast && !exportDone && (
          <div className="absolute bottom-2 left-1/2 -translate-x-1/2 flex items-center gap-2 px-3 py-1.5 bg-gray-800/90 backdrop-blur text-white text-xs rounded-lg shadow-lg z-20 animate-[slideIn_0.2s_ease-out]">
            <svg className="w-3.5 h-3.5 text-green-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
            <span>{t("undo.deleted")}</span>
          </div>
        )}
      </div>
      </div>

      <div className="flex items-center justify-between px-3 h-10 shrink-0 border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800">
        {totalPages > 1 ? (
          <div className="flex items-center gap-0.5">
            <button
              className="w-6 h-6 flex items-center justify-center text-gray-500 hover:text-blue-500 hover:bg-blue-50 rounded disabled:opacity-30 disabled:hover:text-gray-500 disabled:hover:bg-transparent transition-colors"
              onClick={() => goToPage(currentPage - 1)}
              disabled={currentPage === 1}
              title={t("paging.prev")}
            >
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" /></svg>
            </button>
            {getPageNumbers(currentPage, totalPages).map((item, idx) =>
              item === "..." ? (
                <span key={`ellipsis-${idx}`} className="w-6 h-6 flex items-center justify-center text-[11px] text-gray-400">...</span>
              ) : (
                <button
                  key={item}
                  className={`w-6 h-6 flex items-center justify-center text-[11px] rounded transition-colors ${
                    item === currentPage
                      ? "bg-blue-500 text-white font-medium"
                      : "text-gray-600 hover:bg-blue-50 hover:text-blue-500"
                  }`}
                  onClick={() => goToPage(item as number)}
                >
                  {item}
                </button>
              )
            )}
            <button
              className="w-6 h-6 flex items-center justify-center text-gray-500 hover:text-blue-500 hover:bg-blue-50 rounded disabled:opacity-30 disabled:hover:text-gray-500 disabled:hover:bg-transparent transition-colors"
              onClick={() => goToPage(currentPage + 1)}
              disabled={currentPage >= totalPages}
              title={t("paging.next")}
            >
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" /></svg>
            </button>
            <div className="ml-2 flex items-center gap-1">
              <input
                type="text"
                className="w-9 h-6 text-center text-[11px] border border-gray-200 rounded bg-gray-50 focus:bg-white focus:border-blue-400 focus:outline-none"
                placeholder={String(currentPage)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    const val = parseInt((e.target as HTMLInputElement).value);
                    if (!isNaN(val)) { goToPage(val); (e.target as HTMLInputElement).value = ""; }
                  }
                }}
                onBlur={(e) => {
                  const val = parseInt(e.target.value);
                  if (!isNaN(val)) { goToPage(val); e.target.value = ""; }
                }}
              />
              <span className="text-[11px] text-gray-400">/ {totalPages}</span>
            </div>
          </div>
        ) : <div />}
        {totalCount > 0 && !favoritesMode && (
          <button
            className="flex items-center gap-1.5 px-3 py-1 text-xs text-gray-400 hover:text-blue-500 transition-colors"
            onClick={handleExport}
            disabled={exporting}
            title={activeTab === "text" ? t("export.text_tooltip") : t("export.image_tooltip")}
          >
            <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
            </svg>
            {activeTab === "text" ? t("export.text") : t("export.image")}
          </button>
        )}
      </div>
    </div>
  );
}
