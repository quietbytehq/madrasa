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
  /** Release version from the API ("0.1.3-alpha"); empty until loaded. */
  version: string;
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
  const [state, setState] = useState<{
    edition: Edition;
    features: Feature[];
    loaded: boolean;
    version: string;
  }>(
    initial
      ? {
          edition: initial.edition,
          features: initial.features,
          version: initial.version ?? "",
          loaded: true,
        }
      : { edition: "lite", features: [...LITE_FEATURES], version: "", loaded: false },
  );

  const reload = useCallback(
    () =>
      api.features().then(
        (res) =>
          setState({
            edition: res.edition,
            features: res.features,
            version: res.version ?? "",
            loaded: true,
          }),
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
        !cancelled &&
        setState({
          edition: res.edition,
          features: res.features,
          version: res.version ?? "",
          loaded: true,
        }),
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
      version: state.version,
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
