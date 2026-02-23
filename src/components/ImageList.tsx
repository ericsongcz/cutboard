import { useState, useEffect, useCallback, useRef, memo } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ClipboardEntry } from "../App";
import { useTranslation } from "../i18n";

interface Props {
  entries: ClipboardEntry[];
  onDelete: (id: number) => void;
  onCopy: (id: number) => void;
  onToggleFavorite?: (id: number) => void;
}

function ImagePreviewOverlay({ src, onClose }: { src: string; onClose: () => void }) {
  const [scale, setScale] = useState(1);
  const [translate, setTranslate] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const draggingRef = useRef(false);
  const dragStart = useRef({ x: 0, y: 0 });
  const translateStart = useRef({ x: 0, y: 0 });

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.stopPropagation();
    setScale((s) => Math.min(10, Math.max(0.1, s - e.deltaY * 0.002)));
  }, []);

  const translateRef = useRef(translate);
  translateRef.current = translate;

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    draggingRef.current = true;
    setIsDragging(true);
    dragStart.current = { x: e.clientX, y: e.clientY };
    translateStart.current = { ...translateRef.current };
  }, []);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!draggingRef.current) return;
    setTranslate({
      x: translateStart.current.x + e.clientX - dragStart.current.x,
      y: translateStart.current.y + e.clientY - dragStart.current.y,
    });
  }, []);

  const handleMouseUp = useCallback(() => {
    draggingRef.current = false;
    setIsDragging(false);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const handleBackdropClick = useCallback((e: React.MouseEvent) => {
    if (e.target === e.currentTarget) onClose();
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center"
      onClick={handleBackdropClick}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      style={{ cursor: isDragging ? "grabbing" : "default" }}
    >
      <div className="absolute top-4 right-4 flex items-center gap-2 z-10">
        <span className="text-white/70 text-sm bg-black/40 rounded px-2 py-1">
          {Math.round(scale * 100)}%
        </span>
        <button
          className="text-white/70 hover:text-white bg-black/40 hover:bg-black/60 rounded-full p-1.5 transition-colors"
          onClick={onClose}
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
      <img
        src={src}
        alt="full size"
        className="max-h-full max-w-full object-contain rounded-lg shadow-2xl select-none"
        draggable={false}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onClick={(e) => e.stopPropagation()}
        style={{
          transform: `translate(${translate.x}px, ${translate.y}px) scale(${scale})`,
          cursor: isDragging ? "grabbing" : "grab",
          transition: isDragging ? "none" : "transform 0.1s ease-out",
        }}
      />
    </div>
  );
}

const imageCache = new Map<string, string>();
const MAX_IMAGE_CACHE = 100;

function cacheImage(key: string, value: string) {
  if (imageCache.size >= MAX_IMAGE_CACHE) {
    const firstKey = imageCache.keys().next().value;
    if (firstKey) imageCache.delete(firstKey);
  }
  imageCache.set(key, value);
}

const ImageCard = memo(function ImageCard({
  entry,
  onDelete,
  onCopy,
  onToggleFavorite,
  batchSrc,
}: {
  entry: ClipboardEntry;
  onDelete: (id: number) => void;
  onCopy: (id: number) => void;
  onToggleFavorite?: (id: number) => void;
  batchSrc?: string;
}) {
  const { t } = useTranslation();
  const [src, setSrc] = useState<string>(() => {
    if (batchSrc) return batchSrc;
    if (entry.image_path && imageCache.has(entry.image_path)) {
      return imageCache.get(entry.image_path)!;
    }
    return "";
  });
  const [copiedId, setCopiedId] = useState<number | null>(null);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    if (batchSrc) { setSrc(batchSrc); return; }
    if (entry.image_path && imageCache.has(entry.image_path)) {
      setSrc(imageCache.get(entry.image_path)!);
    }
  }, [entry.image_path, batchSrc]);

  const handleCopy = async () => {
    await onCopy(entry.id);
    setCopiedId(entry.id);
    setTimeout(() => setCopiedId(null), 1500);
  };

  return (
    <>
        <div className={`group bg-white rounded-lg border hover:shadow-sm transition-all overflow-hidden dark:bg-gray-800 ${
          entry.is_favorite
            ? "border-amber-300 dark:border-amber-600 border-l-[3px] border-l-amber-400"
            : "border-gray-200 dark:border-gray-700 hover:border-blue-200 dark:hover:border-blue-700"
        }`}>
        <div
          className="p-2 flex items-center justify-center bg-gray-50 dark:bg-gray-900 min-h-[120px] cursor-pointer"
          onClick={() => setExpanded(true)}
        >
          {src ? (
            <img
              src={src}
              alt="clipboard image"
              className="max-h-48 max-w-full object-contain rounded"
              draggable={false}
            />
          ) : (
            <div className="w-full flex flex-col items-center justify-center gap-2 py-4">
              <svg className="w-8 h-8 text-gray-200 dark:text-gray-700" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M2.25 15.75l5.159-5.159a2.25 2.25 0 013.182 0l5.159 5.159m-1.5-1.5l1.409-1.409a2.25 2.25 0 013.182 0l2.909 2.909M3.75 21h16.5A2.25 2.25 0 0022.5 18.75V5.25A2.25 2.25 0 0020.25 3H3.75A2.25 2.25 0 001.5 5.25v13.5A2.25 2.25 0 003.75 21z" />
              </svg>
              <div className="w-24 h-1 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
                <div className="h-full bg-blue-400/60 rounded-full animate-[shimmer_1.5s_ease-in-out_infinite]" />
              </div>
            </div>
          )}
        </div>

        <div className="flex items-center justify-between px-3 py-2 border-t border-gray-100 dark:border-gray-700">
          <span className="text-xs text-gray-400">{entry.created_at}</span>
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
              onClick={handleCopy}
            >
              {copiedId === entry.id ? (
                <>
                  <svg className="w-3.5 h-3.5 text-green-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                  {t("action.copied")}
                </>
              ) : (
                <>
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3" />
                  </svg>
                  {t("action.copy")}
                </>
              )}
            </button>
            <button
              className="flex items-center gap-1 px-2.5 py-1 text-xs rounded-md text-gray-500 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/30 transition-colors"
              onClick={() => onDelete(entry.id)}
            >
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
              {t("action.delete")}
            </button>
          </div>
        </div>
      </div>

      {expanded && src && <ImagePreviewOverlay src={src} onClose={() => setExpanded(false)} />}
    </>
  );
});

export default function ImageList({ entries, onDelete, onCopy, onToggleFavorite }: Props) {
  const [batchImages, setBatchImages] = useState<Record<string, string>>({});

  useEffect(() => {
    const uncached = entries
      .map((e) => e.image_path)
      .filter((p): p is string => !!p && !imageCache.has(p));

    if (uncached.length === 0) {
      const fromCache: Record<string, string> = {};
      entries.forEach((e) => {
        if (e.image_path && imageCache.has(e.image_path)) {
          fromCache[e.image_path] = imageCache.get(e.image_path)!;
        }
      });
      setBatchImages(fromCache);
      return;
    }

    invoke<Record<string, string>>("get_images_base64_batch", { imagePaths: uncached })
      .then((map) => {
        for (const [k, v] of Object.entries(map)) cacheImage(k, v);
        const merged: Record<string, string> = {};
        entries.forEach((e) => {
          if (e.image_path && imageCache.has(e.image_path)) {
            merged[e.image_path] = imageCache.get(e.image_path)!;
          }
        });
        setBatchImages(merged);
      })
      .catch(() => {});
  }, [entries]);

  return (
    <div className="p-3 grid grid-cols-2 gap-3">
      {entries.map((entry) => (
        <ImageCard
          key={entry.id}
          entry={entry}
          onDelete={onDelete}
          onCopy={onCopy}
          onToggleFavorite={onToggleFavorite}
          batchSrc={entry.image_path ? batchImages[entry.image_path] : undefined}
        />
      ))}
    </div>
  );
}
