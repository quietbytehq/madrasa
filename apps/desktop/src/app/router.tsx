import { Navigate, Route, Routes, useLocation } from "react-router-dom";
import type { ReactNode } from "react";
import { AppShell } from "@/layout/AppShell";
import { Dashboard } from "@/pages/Dashboard";
import { ComingSoon } from "@/pages/ComingSoon";
import { Spinner } from "@/components/ui/Spinner";
import { NAV_ITEMS } from "./navigation";
import { useFeatures } from "./features";
import { useSession } from "./session";

function FullPageSpinner() {
  return (
    <div className="flex h-full items-center justify-center">
      <Spinner size="lg" />
    </div>
  );
}

/** Redirects anonymous users to /login, remembering where they wanted to go. */
export function RequireAuth({ children }: { children: ReactNode }) {
  const { status } = useSession();
  const location = useLocation();
  if (status === "loading") return <FullPageSpinner />;
  if (status === "anonymous")
    return <Navigate to="/login" replace state={{ from: location.pathname }} />;
  return <>{children}</>;
}

export interface AppRoutesProps {
  /** Pages supplied by the app (login, setup); kept as props so the shell is testable alone. */
  login: ReactNode;
  setup: ReactNode;
}

export function AppRoutes({ login, setup }: AppRoutesProps) {
  const { has } = useFeatures();
  return (
    <Routes>
      <Route path="/login" element={login} />
      <Route path="/setup" element={setup} />
      <Route
        element={
          <RequireAuth>
            <AppShell />
          </RequireAuth>
        }
      >
        <Route index element={<Dashboard />} />
        {NAV_ITEMS.filter((item) => item.path !== "/" && has(item.feature)).map((item) => (
          <Route
            key={item.path}
            path={item.path}
            element={<ComingSoon labelKey={item.labelKey} />}
          />
        ))}
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
