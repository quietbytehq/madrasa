import { useEffect, useState } from "react";
import { Outlet } from "react-router-dom";
import { useI18n } from "@/i18n";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";

const COLLAPSE_KEY = "madrasa.sidebarCollapsed";

export function AppShell() {
  const { t } = useI18n();
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    try {
      return globalThis.localStorage?.getItem(COLLAPSE_KEY) === "1";
    } catch {
      return false;
    }
  });

  useEffect(() => {
    try {
      globalThis.localStorage?.setItem(COLLAPSE_KEY, collapsed ? "1" : "0");
    } catch {
      // ignore
    }
  }, [collapsed]);

  return (
    <div className="flex h-full min-w-[1024px] overflow-hidden">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:absolute focus:start-2 focus:top-2 focus:z-50 focus:rounded focus:bg-card focus:px-3 focus:py-1.5 focus:text-sm focus:shadow-pop"
      >
        {t("nav.skipToContent")}
      </a>
      <Sidebar collapsed={collapsed} onToggle={() => setCollapsed((c) => !c)} />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar />
        <main id="main" tabIndex={-1} className="scrollbar-thin min-h-0 flex-1 overflow-auto p-4">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
