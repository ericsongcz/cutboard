import { createContext, useContext, useState, useEffect, useCallback } from "react";
import type { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

type LangStrings = Record<string, string>;

interface I18nContextType {
  t: (key: string, params?: Record<string, string>) => string;
  lang: string;
  setLang: (lang: string) => Promise<void>;
}

const I18nContext = createContext<I18nContextType>({
  t: (key) => key,
  lang: "zh-CN",
  setLang: async () => {},
});

export function I18nProvider({ children }: { children: ReactNode }) {
  const [strings, setStrings] = useState<LangStrings>({});
  const [lang, setLangState] = useState("zh-CN");
  const [ready, setReady] = useState(false);

  const loadLanguage = useCallback(async (langCode: string) => {
    try {
      const result = await invoke<LangStrings>("get_language_strings", { lang: langCode });
      setStrings(result);
      setLangState(langCode);
    } catch (e) {
      console.error("Failed to load language:", e);
    }
  }, []);

  useEffect(() => {
    invoke<{ language: string }>("get_settings")
      .then((s) => loadLanguage(s.language || "zh-CN"))
      .catch(() => loadLanguage("zh-CN"))
      .finally(() => setReady(true));
  }, [loadLanguage]);

  const t = useCallback(
    (key: string, params?: Record<string, string>) => {
      let str = strings[key] || key;
      if (params) {
        for (const [k, v] of Object.entries(params)) {
          str = str.replace(`{${k}}`, v);
        }
      }
      return str;
    },
    [strings]
  );

  if (!ready) return null;

  return (
    <I18nContext.Provider value={{ t, lang, setLang: loadLanguage }}>
      {children}
    </I18nContext.Provider>
  );
}

export const useTranslation = () => useContext(I18nContext);
