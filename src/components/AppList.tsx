import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "../App";
import { useTranslation } from "../i18n";

interface Props {
  apps: AppInfo[];
  selectedAppId: number | null;
  showFavorites: boolean;
  onSelect: (id: number) => void;
  onClear: (id: number) => void;
  onOpenSettings: () => void;
  onToggleFavorites: () => void;
  onAppFavToggle: () => void;
}

export default function AppList({ apps, selectedAppId, showFavorites, onSelect, onClear, onOpenSettings, onToggleFavorites, onAppFavToggle }: Props) {
  const { t } = useTranslation();

  const handleToggleAppFav = async (e: React.MouseEvent, id: number) => {
    e.stopPropagation();
    try {
      await invoke("toggle_app_favorite", { id });
      onAppFavToggle();
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <div className="w-56 h-full bg-white dark:bg-gray-800 border-r border-gray-200 dark:border-gray-700 flex flex-col">
      {/* Favorites button at top */}
      <button
        className={`flex items-center gap-3 px-3 mx-1.5 mt-1.5 mb-0.5 py-2.5 rounded-lg border-b-0 cursor-pointer transition-colors ${
          showFavorites ? "bg-amber-50 dark:bg-amber-900/30 text-amber-700 dark:text-amber-400" : "hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-500 dark:text-gray-400"
        }`}
        onClick={onToggleFavorites}
        title={t("favorites.title")}
      >
        <div className="w-8 h-8 rounded-lg bg-amber-50 dark:bg-amber-900/40 flex items-center justify-center flex-shrink-0">
          <svg className="w-5 h-5" fill={showFavorites ? "currentColor" : "none"} viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M11.48 3.499a.562.562 0 011.04 0l2.125 5.111a.563.563 0 00.475.345l5.518.442c.499.04.701.663.321.988l-4.204 3.602a.563.563 0 00-.182.557l1.285 5.385a.562.562 0 01-.84.61l-4.725-2.885a.563.563 0 00-.586 0L6.982 20.54a.562.562 0 01-.84-.61l1.285-5.386a.562.562 0 00-.182-.557l-4.204-3.602a.563.563 0 01.321-.988l5.518-.442a.563.563 0 00.475-.345L11.48 3.5z" />
          </svg>
        </div>
        <span className="text-sm font-medium">{t("favorites.title")}</span>
      </button>
      <div className="border-b border-gray-200 dark:border-gray-700 mx-3 mb-0.5"></div>

      <div className="flex-1 overflow-y-auto py-1">
        {apps.length === 0 ? (
          <div className="px-4 py-8 text-center text-gray-400 text-sm">
            {t("sidebar.no_records")}
          </div>
        ) : (
          apps.map((app) => (
            <div
              key={app.id}
              className={`group flex items-center gap-3 px-3 py-2.5 mx-1.5 my-0.5 rounded-lg cursor-pointer transition-colors ${
                selectedAppId === app.id && !showFavorites
                  ? "bg-blue-50 dark:bg-blue-900/30 text-blue-700 dark:text-blue-400"
                  : app.is_favorite
                    ? "bg-amber-50/60 dark:bg-amber-900/20 hover:bg-amber-50 dark:hover:bg-amber-900/30 text-gray-700 dark:text-gray-300 border border-amber-200/60 dark:border-amber-700/40"
                    : "hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300"
              }`}
              onClick={() => onSelect(app.id)}
            >
              <div className="w-8 h-8 rounded-lg bg-gray-100 dark:bg-gray-700 flex items-center justify-center flex-shrink-0 overflow-hidden">
                {app.icon_base64 ? (
                  <img
                    src={`data:image/png;base64,${app.icon_base64}`}
                    alt={app.name}
                    className="w-6 h-6"
                    draggable={false}
                  />
                ) : (
                  <span className="text-xs font-bold text-gray-400">
                    {app.name.charAt(0).toUpperCase()}
                  </span>
                )}
              </div>

              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium truncate">{app.name}</div>
                <div className="text-xs text-gray-400">{t("sidebar.entry_count", { count: String(app.entry_count) })}</div>
              </div>

              <div className="flex items-center gap-0.5">
                <button
                  className={`p-1 rounded transition-all ${
                    app.is_favorite
                      ? "text-amber-500"
                      : "opacity-0 group-hover:opacity-100 text-gray-400 hover:text-amber-500"
                  }`}
                  title={t("favorites.toggle")}
                  onClick={(e) => handleToggleAppFav(e, app.id)}
                >
                  <svg className="w-3.5 h-3.5" fill={app.is_favorite ? "currentColor" : "none"} viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M11.48 3.499a.562.562 0 011.04 0l2.125 5.111a.563.563 0 00.475.345l5.518.442c.499.04.701.663.321.988l-4.204 3.602a.563.563 0 00-.182.557l1.285 5.385a.562.562 0 01-.84.61l-4.725-2.885a.563.563 0 00-.586 0L6.982 20.54a.562.562 0 01-.84-.61l1.285-5.386a.562.562 0 00-.182-.557l-4.204-3.602a.563.563 0 01.321-.988l5.518-.442a.563.563 0 00.475-.345L11.48 3.5z" />
                  </svg>
                </button>
                <button
                  className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-red-100 dark:hover:bg-red-900/30 text-gray-400 hover:text-red-500 transition-all"
                  title={t("sidebar.clear_records")}
                  onClick={(e) => {
                    e.stopPropagation();
                    onClear(app.id);
                  }}
                >
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                  </svg>
                </button>
              </div>
            </div>
          ))
        )}
      </div>

      <div className="px-3 h-10 flex items-center border-t border-gray-200 dark:border-gray-700">
        <button
          className="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
          onClick={onOpenSettings}
          title={t("settings.title")}
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
      </div>
    </div>
  );
}
