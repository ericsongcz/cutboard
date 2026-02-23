import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";

export interface SourceInfo {
  domain: string;
  count: number;
}

interface Props {
  sources: SourceInfo[];
  selectedDomain: string | null;
  onSelect: (domain: string | null) => void;
  onDeleteDomain?: (domain: string) => void;
}

const COLORS = [
  "#3B82F6", "#EF4444", "#10B981", "#F59E0B", "#8B5CF6",
  "#EC4899", "#06B6D4", "#84CC16", "#F97316", "#6366F1",
];

function getDomainColor(domain: string): string {
  let hash = 0;
  for (let i = 0; i < domain.length; i++) {
    hash = domain.charCodeAt(i) + ((hash << 5) - hash);
  }
  return COLORS[Math.abs(hash) % COLORS.length];
}

const STATIC_FAVICON_SOURCES = [
  (d: string) => `https://${d}/favicon.ico`,
  (d: string) => `https://icons.duckduckgo.com/ip3/${d}.ico`,
  (d: string) => `https://favicon.cccyun.cc/${d}`,
  (d: string) => `https://favicon.yandex.net/favicon/v2/${encodeURIComponent(`https://${d}`)}?size=32`,
  (d: string) => `https://api.faviconkit.com/${d}/64`,
  (d: string) => `https://www.google.com/s2/favicons?domain=${encodeURIComponent(d)}&sz=64`,
];

const CACHE_KEY = "cutboard_favicon_cache";
const RESOLVED_CACHE_KEY = "cutboard_favicon_resolved";
const MAX_FAVICON_CACHE = 200;

let _faviconCache: Record<string, string> | null = null;
let _resolvedCache: Record<string, string> | null = null;

function loadCache(): Record<string, string> {
  if (_faviconCache) return _faviconCache;
  try {
    _faviconCache = JSON.parse(localStorage.getItem(CACHE_KEY) || "{}");
  } catch {
    _faviconCache = {};
  }
  return _faviconCache!;
}

function saveCache(cache: Record<string, string>) {
  const keys = Object.keys(cache);
  if (keys.length > MAX_FAVICON_CACHE) {
    const toRemove = keys.slice(0, keys.length - MAX_FAVICON_CACHE);
    for (const k of toRemove) delete cache[k];
  }
  _faviconCache = cache;
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
  } catch { /* ignore */ }
}

function loadResolvedCache(): Record<string, string> {
  if (_resolvedCache) return _resolvedCache;
  try {
    _resolvedCache = JSON.parse(localStorage.getItem(RESOLVED_CACHE_KEY) || "{}");
  } catch {
    _resolvedCache = {};
  }
  return _resolvedCache!;
}

function saveResolvedCache(cache: Record<string, string>) {
  const keys = Object.keys(cache);
  if (keys.length > MAX_FAVICON_CACHE) {
    const toRemove = keys.slice(0, keys.length - MAX_FAVICON_CACHE);
    for (const k of toRemove) delete cache[k];
  }
  _resolvedCache = cache;
  try {
    localStorage.setItem(RESOLVED_CACHE_KEY, JSON.stringify(cache));
  } catch { /* ignore */ }
}

function DomainIcon({ domain, selected }: { domain: string; selected: boolean }) {
  const cachedUrl = loadCache()[domain];
  const [currentSrc, setCurrentSrc] = useState<string>(cachedUrl || STATIC_FAVICON_SOURCES[0](domain));
  const [failed, setFailed] = useState(false);
  const staticIndexRef = useRef(0);
  const triedBackendRef = useRef(false);

  useEffect(() => {
    staticIndexRef.current = 0;
    triedBackendRef.current = false;
    setFailed(false);
    const cached = loadCache()[domain];
    setCurrentSrc(cached || STATIC_FAVICON_SOURCES[0](domain));
  }, [domain]);

  const tryNextSource = useCallback(async () => {
    staticIndexRef.current++;

    if (staticIndexRef.current < STATIC_FAVICON_SOURCES.length) {
      setCurrentSrc(STATIC_FAVICON_SOURCES[staticIndexRef.current](domain));
      return;
    }

    if (!triedBackendRef.current) {
      triedBackendRef.current = true;
      const resolved = loadResolvedCache()[domain];
      if (resolved) {
        setCurrentSrc(resolved);
        return;
      }
      try {
        const url = await invoke<string>("resolve_favicon", { domain });
        if (url) {
          const rc = loadResolvedCache();
          rc[domain] = url;
          saveResolvedCache(rc);
          setCurrentSrc(url);
          return;
        }
      } catch { /* backend also failed */ }
    }

    setFailed(true);
  }, [domain]);

  const handleError = useCallback(() => {
    tryNextSource();
  }, [tryNextSource]);

  const handleLoad = useCallback(() => {
    const cache = loadCache();
    if (cache[domain] !== currentSrc) {
      cache[domain] = currentSrc;
      saveCache(cache);
    }
  }, [domain, currentSrc]);

  const ringClass = selected ? "ring-2 ring-blue-400 ring-offset-1" : "";

  if (failed) {
    return (
      <div
        className={`w-9 h-9 rounded-full flex items-center justify-center text-white text-sm font-bold ${ringClass}`}
        style={{ backgroundColor: getDomainColor(domain) }}
      >
        {domain.charAt(0).toUpperCase()}
      </div>
    );
  }

  return (
    <div className={`w-9 h-9 rounded-full overflow-hidden bg-gray-100 flex items-center justify-center ${ringClass}`}>
      <img
        src={currentSrc}
        alt={domain}
        className="w-6 h-6"
        onError={handleError}
        onLoad={handleLoad}
      />
    </div>
  );
}

export default function SourceList({ sources, selectedDomain, onSelect, onDeleteDomain }: Props) {
  const { t } = useTranslation();
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; domain: string } | null>(null);

  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [contextMenu]);

  const handleContextMenu = (e: React.MouseEvent, domain: string) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY, domain });
  };

  return (
    <div className="w-[72px] shrink-0 border-r border-gray-200 dark:border-gray-700 overflow-y-auto flex flex-col items-center py-2 gap-1 bg-white dark:bg-gray-800">
      <button
        className={`flex flex-col items-center gap-0.5 p-1.5 rounded-lg w-[64px] transition-colors ${
          selectedDomain === null ? "bg-blue-50" : "hover:bg-gray-50"
        }`}
        onClick={() => onSelect(null)}
        title={t("sources.all")}
      >
        <div
          className={`w-9 h-9 rounded-full flex items-center justify-center text-xs font-bold ${
            selectedDomain === null
              ? "bg-blue-500 text-white"
              : "bg-gray-200 text-gray-500"
          }`}
        >
          All
        </div>
      </button>

      {sources.map((source) => (
        <button
          key={source.domain}
          className={`flex flex-col items-center gap-0.5 p-1.5 rounded-lg w-[64px] transition-colors ${
            selectedDomain === source.domain ? "bg-blue-50" : "hover:bg-gray-50"
          }`}
          onClick={() => onSelect(source.domain)}
          onContextMenu={(e) => handleContextMenu(e, source.domain)}
          title={`${source.domain} (${source.count})`}
        >
          <DomainIcon domain={source.domain} selected={selectedDomain === source.domain} />
          <span className="text-[10px] text-gray-500 w-full text-center truncate leading-tight">
            {source.domain}
          </span>
        </button>
      ))}

      {contextMenu && (
        <div
          className="fixed z-50 bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700 py-1 min-w-[120px]"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          <button
            className="w-full flex items-center gap-2 px-3 py-2 text-sm text-red-600 hover:bg-red-50 transition-colors"
            onClick={() => {
              onDeleteDomain?.(contextMenu.domain);
              setContextMenu(null);
            }}
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
            {t("action.delete")}
          </button>
        </div>
      )}
    </div>
  );
}
