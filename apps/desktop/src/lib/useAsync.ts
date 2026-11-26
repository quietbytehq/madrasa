import { useCallback, useEffect, useRef, useState } from "react";
import { isLocale, translate } from "@/i18n";
import type { TranslationKey } from "@/i18n";

export interface AsyncState<T> {
  data: T | null;
  error: unknown;
  loading: boolean;
  /** Re-run the loader. */
  reload: () => Promise<void>;
  /** Replace the data locally (after a mutation). */
  setData: (data: T | null) => void;
}

/**
 * Minimal data-loading hook: runs `loader` on mount and whenever `deps` change, ignoring
 * results from superseded runs.
 */
export function useAsync<T>(loader: () => Promise<T>, deps: readonly unknown[]): AsyncState<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const run = useRef(0);
  const loaderRef = useRef(loader);
  loaderRef.current = loader;

  const reload = useCallback(async () => {
    const id = ++run.current;
    setLoading(true);
    try {
      const result = await loaderRef.current();
      if (id === run.current) {
        setData(result);
        setError(null);
      }
    } catch (err) {
      if (id === run.current) setError(err);
    } finally {
      if (id === run.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return { data, error, loading, reload, setData };
}

/**
 * Errors whose words are the same wherever they happen. The server says them in English;
 * the app says them in the person's language.
 */
const SAID_BY_THE_APP: Record<string, TranslationKey> = {
  internal: "app.error.server",
  forbidden: "app.error.forbidden",
  unauthenticated: "app.error.signedOut",
};

/** Human message for an API/network failure. */
export function errorMessage(err: unknown, fallback: string): string {
  if (err && typeof err === "object" && "code" in err && typeof err.code === "string") {
    const key = SAID_BY_THE_APP[err.code];
    const lang = typeof document === "undefined" ? "en" : document.documentElement.lang;
    if (key) return translate(isLocale(lang) ? lang : "en", key);
  }
  if (
    err &&
    typeof err === "object" &&
    "message" in err &&
    typeof err.message === "string" &&
    err.message !== "network"
  ) {
    return err.message;
  }
  return fallback;
}
