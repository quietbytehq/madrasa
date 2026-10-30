import { createContext, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { Navigate, useLocation } from "react-router-dom";
import { api } from "@/api/endpoints";
import { Spinner } from "@/components/ui/Spinner";

/**
 * On start, asks the API whether first-run setup is still needed and routes to /setup if so.
 * Any error (API not up yet, network) falls through to the normal flow so the login page can
 * show a useful message.
 */
interface SetupGateContextValue {
  /** Called by the wizard once setup has completed so the gate stops redirecting to /setup. */
  markCompleted: () => void;
}

const SetupGateContext = createContext<SetupGateContextValue>({ markCompleted: () => {} });

export function useSetupGate(): SetupGateContextValue {
  return useContext(SetupGateContext);
}

export function SetupGate({ children, skip }: { children: ReactNode; skip?: boolean }) {
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(skip ? false : null);
  // Set when the wizard finishes in this session: the wizard keeps showing its last page
  // (the recovery key) and navigates to /login itself once the user has acknowledged it.
  const [completedHere, setCompletedHere] = useState(false);
  const location = useLocation();
  const ctx = useMemo<SetupGateContextValue>(
    () => ({
      markCompleted: () => {
        setCompletedHere(true);
        setNeedsSetup(false);
      },
    }),
    [],
  );

  useEffect(() => {
    if (skip) return;
    let cancelled = false;
    api
      .setupStatus()
      .then((s) => !cancelled && setNeedsSetup(s.needs_setup))
      .catch(() => !cancelled && setNeedsSetup(false));
    return () => {
      cancelled = true;
    };
  }, [skip]);

  if (needsSetup === null) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner size="lg" />
      </div>
    );
  }
  if (needsSetup && location.pathname !== "/setup") return <Navigate to="/setup" replace />;
  if (!needsSetup && !completedHere && location.pathname === "/setup")
    return <Navigate to="/login" replace />;
  return <SetupGateContext.Provider value={ctx}>{children}</SetupGateContext.Provider>;
}
