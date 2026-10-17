// The UI's two languages (spec §4.5). The app opens in English; Thai is a choice on the
// wizard's first page and in Settings, which the app remembers (founder 2026-10-16). The
// language is React state here, so a switch re-renders every screen at once.
import { createContext, useCallback, useContext, useMemo, type ReactNode } from "react";
import { en } from "./en";
import { th } from "./th";
import type { Dictionary, Key, Language } from "./types";

export type { Dictionary, Key, Language };

export const DICTIONARIES: Record<Language, Dictionary> = { en, th };

/** Fills `{name}` holes; a hole with no value stays as written (the welcome box's own
 * variables, such as `{user}` in a help text, are meant to show). */
export function fill(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => {
    // Own properties only: `{constructor}` must not be filled from Object.prototype.
    if (!Object.hasOwn(vars, name)) return whole;
    const value = vars[name];
    return value === undefined ? whole : String(value);
  });
}

export type Translate = (key: Key, vars?: Record<string, string | number>) => string;

interface I18nValue {
  language: Language;
  setLanguage: (language: Language) => void;
  t: Translate;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({
  language,
  setLanguage,
  children,
}: {
  language: Language;
  setLanguage: (language: Language) => void;
  children: ReactNode;
}) {
  // A key missing at run time (a stale build) falls back to English, then to the key itself;
  // the types and i18n.test.ts keep it from happening, and a screen must never throw for it.
  const t = useCallback<Translate>(
    (key, vars) => fill(DICTIONARIES[language][key] ?? en[key] ?? key, vars),
    [language],
  );
  const value = useMemo(() => ({ language, setLanguage, t }), [language, setLanguage, t]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useI18n needs an I18nProvider");
  return value;
}

export function useT(): Translate {
  return useI18n().t;
}

/** A clock time or a date and time in the UI's language (Thai shows the Buddhist year). */
export function formatTime(ms: number, language: Language, withDate = true): string {
  const locale = language === "th" ? "th-TH" : "en-GB";
  const options: Intl.DateTimeFormatOptions = withDate
    ? { dateStyle: "medium", timeStyle: "short" }
    : { timeStyle: "short" };
  return new Intl.DateTimeFormat(locale, options).format(new Date(ms));
}

/** The language's own text of a bilingual value from the Rust side. */
export function pick(language: Language, th: string, en: string): string {
  return language === "th" && th.length > 0 ? th : en;
}
