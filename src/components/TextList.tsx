import { useState, useMemo, useRef, useEffect, useCallback, memo } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ClipboardEntry } from "../App";
import { useTranslation } from "../i18n";

const textImageCache = new Map<string, string>();

function cacheTextImage(key: string, value: string) {
  if (textImageCache.size >= 50) {
    const firstKey = textImageCache.keys().next().value;
    if (firstKey) textImageCache.delete(firstKey);
  }
  textImageCache.set(key, value);
}

interface Props {
  entries: ClipboardEntry[];
  onDelete: (id: number) => void;
  onCopy: (id: number) => void;
  onToggleFavorite?: (id: number) => void;
  onToggleSensitive?: (id: number) => void;
}

const TEXT_PAGE_SIZE = 500;

function sanitizeHtml(html: string): string {
  const div = document.createElement("div");
  div.innerHTML = html;
  div.querySelectorAll("script,style,iframe,object,embed,form,input,link,img").forEach((el) => el.remove());
  div.querySelectorAll("*").forEach((el) => {
    for (const attr of Array.from(el.attributes)) {
      if (attr.name.startsWith("on") || attr.name === "style") {
        el.removeAttribute(attr.name);
      }
      if (attr.name === "href" || attr.name === "src") {
        if (attr.value.toLowerCase().startsWith("javascript:")) {
          el.removeAttribute(attr.name);
        }
      }
    }
  });
  return div.innerHTML;
}

const TextCard = memo(function TextCard({
  entry,
  copiedId,
  onCopy,
  onDelete,
  onToggleFavorite,
  onToggleSensitive,
}: {
  entry: ClipboardEntry;
  copiedId: number | null;
  onCopy: (id: number) => void;
  onDelete: (id: number) => void;
  onToggleFavorite?: (id: number) => void;
  onToggleSensitive?: (id: number) => void;
}) {
  const { t } = useTranslation();
  const text = entry.text_content || "";
  const totalPages = Math.max(1, Math.ceil(text.length / TEXT_PAGE_SIZE));
  const needsPaging = totalPages > 1;
  const [page, setPage] = useState(1);
  const [showSensitive, setShowSensitive] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null);
  const ctxRef = useRef<HTMLDivElement>(null);
  const [imgSrc, setImgSrc] = useState<string>(() =>
    entry.image_path && textImageCache.has(entry.image_path) ? textImageCache.get(entry.image_path)! : ""
  );
  const [imgExpanded, setImgExpanded] = useState(false);

  useEffect(() => {
    if (!entry.image_path || textImageCache.has(entry.image_path)) return;
    invoke<string>("get_image_base64", { imagePath: entry.image_path })
      .then((b64) => {
        const src = `data:image/png;base64,${b64}`;
        cacheTextImage(entry.image_path!, src);
        setImgSrc(src);
      })
      .catch(() => {});
  }, [entry.image_path]);

  const hasHtml = !!entry.html_content;

  const sanitizedHtml = useMemo(
    () => (entry.html_content ? sanitizeHtml(entry.html_content) : ""),
    [entry.html_content]
  );

  const displayText = useMemo(() => {
    if (!needsPaging) return text;
    const start = (page - 1) * TEXT_PAGE_SIZE;
    return text.slice(start, start + TEXT_PAGE_SIZE);
  }, [text, page, needsPaging]);

  const maskedText = entry.is_sensitive && !showSensitive;

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY });
  }, []);

  useEffect(() => {
    if (!ctxMenu) return;
    const close = (e: MouseEvent) => {
      if (ctxRef.current && !ctxRef.current.contains(e.target as Node)) setCtxMenu(null);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [ctxMenu]);

  return (
    <div
      className={`group bg-white dark:bg-gray-800 rounded-lg border hover:shadow-sm transition-all relative ${
        entry.is_favorite
          ? "border-amber-300 dark:border-amber-600 border-l-[3px] border-l-amber-400"
          : "border-gray-200 dark:border-gray-700 hover:border-blue-200 dark:hover:border-blue-700"
      }`}
      onContextMenu={handleContextMenu}
    >
      {ctxMenu && (
        <div
          ref={ctxRef}
          className="fixed z-50 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-600 rounded-lg shadow-lg py-1 min-w-[160px]"
          style={{ left: ctxMenu.x, top: ctxMenu.y }}
        >
          <button
            className="w-full px-3 py-1.5 text-xs text-left hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 flex items-center gap-2"
            onClick={() => { onToggleSensitive?.(entry.id); setCtxMenu(null); }}
          >
            {entry.is_sensitive ? (
              <>
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 11V7a4 4 0 118 0m-4 8v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2z" /></svg>
                {t("sensitive.unmark")}
              </>
            ) : (
              <>
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" /></svg>
                {t("sensitive.mark")}
              </>
            )}
          </button>
        </div>
      )}

      {imgSrc && !maskedText && (
        <div className="px-3 pt-3">
          <img
            src={imgSrc}
            alt=""
            className={`rounded cursor-pointer transition-all ${imgExpanded ? "max-h-none w-full" : "max-h-32 w-auto"} object-contain`}
            onClick={() => setImgExpanded(!imgExpanded)}
          />
        </div>
      )}

      <div className="p-3 flex gap-2">
        {entry.is_sensitive && (
          <button
            className="mt-0.5 shrink-0 text-gray-400 hover:text-amber-500 transition-colors"
            onClick={() => setShowSensitive(!showSensitive)}
            title={showSensitive ? t("sensitive.hide") : t("sensitive.show")}
          >
            {showSensitive ? (
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>
            ) : (
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" /></svg>
            )}
          </button>
        )}
        <div className="flex-1 min-w-0">
          {maskedText ? (
            <div className="text-sm text-gray-400 italic select-none">••••••••••••••••</div>
          ) : hasHtml ? (
            <div
              className="text-sm text-gray-700 dark:text-gray-300 leading-relaxed max-h-40 overflow-y-auto select-text cursor-text prose prose-sm dark:prose-invert max-w-none"
              dangerouslySetInnerHTML={{ __html: sanitizedHtml }}
            />
          ) : (
            <pre className="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap break-words font-sans leading-relaxed max-h-40 overflow-y-auto select-text cursor-text">
              {displayText}
            </pre>
          )}
        </div>
      </div>

      {needsPaging && !maskedText && !hasHtml && (
        <div className="flex items-center justify-center gap-1 px-3 py-1.5 border-t border-gray-100 dark:border-gray-700">
          <button className="w-5 h-5 flex items-center justify-center text-gray-400 hover:text-blue-500 rounded disabled:opacity-30 transition-colors" onClick={() => setPage(1)} disabled={page === 1}>
            <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 19l-7-7 7-7m8 14l-7-7 7-7" /></svg>
          </button>
          <button className="w-5 h-5 flex items-center justify-center text-gray-400 hover:text-blue-500 rounded disabled:opacity-30 transition-colors" onClick={() => setPage((p) => Math.max(1, p - 1))} disabled={page === 1}>
            <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" /></svg>
          </button>
          <span className="text-[10px] text-gray-400 px-1.5 select-none">{page} / {totalPages}</span>
          <button className="w-5 h-5 flex items-center justify-center text-gray-400 hover:text-blue-500 rounded disabled:opacity-30 transition-colors" onClick={() => setPage((p) => Math.min(totalPages, p + 1))} disabled={page >= totalPages}>
            <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" /></svg>
          </button>
          <button className="w-5 h-5 flex items-center justify-center text-gray-400 hover:text-blue-500 rounded disabled:opacity-30 transition-colors" onClick={() => setPage(totalPages)} disabled={page >= totalPages}>
            <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 5l7 7-7 7M5 5l7 7-7 7" /></svg>
          </button>
          <span className="text-[10px] text-gray-300 ml-1">{text.length.toLocaleString()} chars</span>
        </div>
      )}

      <div className="flex items-center justify-between px-3 py-2 border-t border-gray-100 dark:border-gray-700 bg-gray-50/50 dark:bg-gray-800/50 rounded-b-lg">
        <div className="flex items-center gap-2">
          <span className="text-xs text-gray-400">{entry.created_at}</span>
          {!maskedText && (
            <span className="text-[10px] text-gray-300 dark:text-gray-500">{text.length.toLocaleString()} {t("stats.chars")}</span>
          )}
        </div>
        <div className="flex items-center gap-1">
          {onToggleFavorite && (
            <button
              className={`p-1 rounded-md transition-colors ${entry.is_favorite ? "text-amber-500" : "text-gray-400 hover:text-amber-500"}`}
              onClick={() => onToggleFavorite(entry.id)}
            >
              <svg className="w-3.5 h-3.5" fill={entry.is_favorite ? "currentColor" : "none"} viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M11.48 3.499a.562.562 0 011.04 0l2.125 5.111a.563.563 0 00.475.345l5.518.442c.499.04.701.663.321.988l-4.204 3.602a.563.563 0 00-.182.557l1.285 5.385a.562.562 0 01-.84.61l-4.725-2.885a.563.563 0 00-.586 0L6.982 20.54a.562.562 0 01-.84-.61l1.285-5.386a.562.562 0 00-.182-.557l-4.204-3.602a.563.563 0 01.321-.988l5.518-.442a.563.563 0 00.475-.345L11.48 3.5z" />
              </svg>
            </button>
          )}
          <button
            className="flex items-center gap-1 px-2.5 py-1 text-xs rounded-md text-gray-500 hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/30 transition-colors"
            onClick={() => onCopy(entry.id)}
          >
            {copiedId === entry.id ? (
              <>
                <svg className="w-3.5 h-3.5 text-green-500" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" /></svg>
                {t("action.copied")}
              </>
            ) : (
              <>
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3" /></svg>
                {t("action.copy")}
              </>
            )}
          </button>
          <button
            className="flex items-center gap-1 px-2.5 py-1 text-xs rounded-md text-gray-500 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/30 transition-colors"
            onClick={() => onDelete(entry.id)}
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
            {t("action.delete")}
          </button>
        </div>
      </div>
    </div>
  );
});

export default function TextList({ entries, onDelete, onCopy, onToggleFavorite, onToggleSensitive }: Props) {
  const [copiedId, setCopiedId] = useState<number | null>(null);

  const handleCopy = async (id: number) => {
    await onCopy(id);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 1500);
  };

  return (
    <div className="p-3 space-y-2">
      {entries.map((entry) => (
        <TextCard
          key={entry.id}
          entry={entry}
          copiedId={copiedId}
          onCopy={handleCopy}
          onDelete={onDelete}
          onToggleFavorite={onToggleFavorite}
          onToggleSensitive={onToggleSensitive}
        />
      ))}
    </div>
  );
}
