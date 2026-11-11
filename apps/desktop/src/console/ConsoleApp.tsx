import { useCallback, useEffect, useMemo, useState } from "react";
import { BrowserRouter } from "react-router-dom";
import { ToastProvider } from "@/components/ui/Toast";
import { Spinner } from "@/components/ui/Spinner";
import { Button } from "@/components/ui/Button";
import { I18nProvider, useI18n } from "@/i18n";
import { ThemeProvider } from "@/theme";
import { getHubToken, hub, setHubToken, setSignedOutHandler } from "./api";
import type { HubPermission, Me, SetupStatus } from "./api";
import { HubSessionContext } from "./session";
import type { HubSession } from "./session";
import { Shell } from "./Shell";
import { LoginPage } from "./pages/LoginPage";
import { SetupPage } from "./pages/SetupPage";

type Stage =
  | { at: "loading" }
  | { at: "unreachable" }
  | { at: "setup" }
  | { at: "login"; hubName: string | null }
  | { at: "in"; me: Me };

/**
 * The hub console: first-run setup while the hub has no users, then sign-in, then the
 * pages. Everything shown comes from the hub's API, which checks every permission itself.
 */
export function Gate() {
  const { t } = useI18n();
  const [stage, setStage] = useState<Stage>({ at: "loading" });

  const load = useCallback(async () => {
    let status: SetupStatus;
    try {
      status = await hub.setupStatus();
    } catch {
      return setStage({ at: "unreachable" });
    }
    if (status.needs_setup) return setStage({ at: "setup" });
    if (getHubToken()) {
      try {
        return setStage({ at: "in", me: await hub.me() });
      } catch {
        setHubToken(null);
      }
    }
    setStage({ at: "login", hubName: status.hub_name });
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const signOut = useCallback(() => {
    setHubToken(null);
    void load();
  }, [load]);

  // A session that ended at the hub (expired, disabled) sends the user back to sign in.
  useEffect(() => {
    setSignedOutHandler(signOut);
    return () => setSignedOutHandler(null);
  }, [signOut]);

  const session = useMemo<HubSession | null>(() => {
    if (stage.at !== "in") return null;
    const me = stage.me;
    return {
      me,
      can: (p: HubPermission) => me.user.permissions.includes(p),
      signOut: () => {
        void hub.logout().catch(() => undefined);
        signOut();
      },
    };
  }, [stage, signOut]);

  if (stage.at === "loading")
    return (
      <div className="flex h-screen items-center justify-center">
        <Spinner />
      </div>
    );
  if (stage.at === "unreachable")
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-3 p-6 text-center">
        <p role="alert" className="text-sm text-text">
          {t("hub.error.network")}
        </p>
        <Button onClick={() => void load()}>{t("action.retry")}</Button>
      </div>
    );
  if (stage.at === "setup") return <SetupPage onDone={() => void load()} />;
  if (stage.at === "login")
    return <LoginPage hubName={stage.hubName} onSignedIn={() => void load()} />;
  return (
    <HubSessionContext.Provider value={session}>
      <Shell />
    </HubSessionContext.Provider>
  );
}

export function ConsoleApp() {
  return (
    <BrowserRouter>
      <ThemeProvider>
        <I18nProvider>
          <ToastProvider>
            <Gate />
          </ToastProvider>
        </I18nProvider>
      </ThemeProvider>
    </BrowserRouter>
  );
}
