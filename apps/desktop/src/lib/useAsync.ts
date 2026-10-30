import { useCallback, useEffect, useRef, useState } from "react";

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

/** Human message for an API/network failure. */
export function errorMessage(err: unknown, fallback: string): string {
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
