import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { api } from "@/api/endpoints";
import type { Edition, Feature, FeaturesResponse } from "@/api/types";

/**
 * Edition feature flags as reported by the API (`GET /system/features`). The backend is the
 * authority; the UI only uses this to hide navigation and show "Coming soon" states.
 */
export interface Features {
  edition: Edition;
  features: ReadonlySet<Feature>;
  loaded: boolean;
  has: (feature: Feature) => boolean;
  reload: () => Promise<void>;
}

const FeaturesContext = createContext<Features | null>(null);

/** Default used before the API answers (and in tests): Lite. Must match madrasa_core. */
export const LITE_FEATURES: readonly Feature[] = [
  "dashboard",
  "students",
  "attendance",
  "examinations",
  "fees",
  "teachers",
  "reports",
  "backup",
  "settings",
];

export function FeaturesProvider({
  children,
  initial,
}: {
  children: ReactNode;
  /** Skip the network call and use this (tests, storybook). */
  initial?: FeaturesResponse;
}) {
  const [state, setState] = useState<{ edition: Edition; features: Feature[]; loaded: boolean }>(
    initial
      ? { ...initial, loaded: true }
      : { edition: "lite", features: [...LITE_FEATURES], loaded: false },
  );

  const reload = useCallback(
    () =>
      api.features().then(
        (res) => setState({ edition: res.edition, features: res.features, loaded: true }),
        // Keep the Lite defaults; the backend still enforces everything.
        () => setState((s) => ({ ...s, loaded: true })),
      ),
    [],
  );

  useEffect(() => {
    if (initial) return;
    let cancelled = false;
    api.features().then(
      (res) =>
        !cancelled && setState({ edition: res.edition, features: res.features, loaded: true }),
      () => !cancelled && setState((s) => ({ ...s, loaded: true })),
    );
    return () => {
      cancelled = true;
    };
  }, [initial]);

  const value = useMemo<Features>(() => {
    const set = new Set(state.features);
    return {
      edition: state.edition,
      features: set,
      loaded: state.loaded,
      has: (f) => set.has(f),
      reload,
    };
  }, [state, reload]);

  return <FeaturesContext.Provider value={value}>{children}</FeaturesContext.Provider>;
}

export function useFeatures(): Features {
  const ctx = useContext(FeaturesContext);
  if (!ctx) throw new Error("useFeatures must be used inside <FeaturesProvider>");
  return ctx;
}
