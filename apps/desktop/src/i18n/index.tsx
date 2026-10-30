import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { en } from "./en";
import type { Translation, TranslationKey } from "./en";
import { ur } from "./ur";

export type Locale = "en" | "ur";
export type Direction = "ltr" | "rtl";

export const LOCALES: readonly Locale[] = ["en", "ur"] as const;
export const DEFAULT_LOCALE: Locale = "en";
const STORAGE_KEY = "madrasa.locale";

const dictionaries: Record<Locale, Translation> = { en, ur };

export function directionFor(locale: Locale): Direction {
  return locale === "ur" ? "rtl" : "ltr";
}

export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && (LOCALES as readonly string[]).includes(value);
}

/** Replace `{name}` placeholders. Missing params are left in place so they are visible. */
export function interpolate(template: string, params?: Record<string, string | number>): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, key: string) =>
    key in params ? String(params[key]) : match,
  );
}

export function translate(
  locale: Locale,
  key: TranslationKey,
  params?: Record<string, string | number>,
): string {
  const template = dictionaries[locale][key] ?? dictionaries.en[key] ?? key;
  return interpolate(template, params);
}

export type TFunction = (key: TranslationKey, params?: Record<string, string | number>) => string;

export interface I18n {
  locale: Locale;
  dir: Direction;
  setLocale: (locale: Locale) => void;
  t: TFunction;
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  formatDate: (value: Date | string, options?: Intl.DateTimeFormatOptions) => string;
}

const intlLocale: Record<Locale, string> = { en: "en-PK", ur: "ur-PK" };

function readStoredLocale(): Locale {
  try {
    const stored = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (isLocale(stored)) return stored;
  } catch {
    // Storage may be unavailable (private mode, tests); fall through.
  }
  return DEFAULT_LOCALE;
}

export function applyDocumentLocale(locale: Locale) {
  if (typeof document === "undefined") return;
  document.documentElement.lang = locale;
  document.documentElement.dir = directionFor(locale);
}

const I18nContext = createContext<I18n | null>(null);

export function I18nProvider({
  children,
  initialLocale,
}: {
  children: ReactNode;
  initialLocale?: Locale;
}) {
  const [locale, setLocaleState] = useState<Locale>(initialLocale ?? readStoredLocale);

  useEffect(() => {
    applyDocumentLocale(locale);
    try {
      globalThis.localStorage?.setItem(STORAGE_KEY, locale);
    } catch {
      // ignore
    }
  }, [locale]);

  const setLocale = useCallback((next: Locale) => setLocaleState(next), []);

  const value = useMemo<I18n>(() => {
    const tag = intlLocale[locale];
    return {
      locale,
      dir: directionFor(locale),
      setLocale,
      t: (key, params) => translate(locale, key, params),
      formatNumber: (n, options) => new Intl.NumberFormat(tag, options).format(n),
      formatDate: (d, options) =>
        new Intl.DateTimeFormat(tag, options ?? { dateStyle: "medium" }).format(
          typeof d === "string" ? new Date(d) : d,
        ),
    };
  }, [locale, setLocale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used inside <I18nProvider>");
  return ctx;
}

export type { TranslationKey };
