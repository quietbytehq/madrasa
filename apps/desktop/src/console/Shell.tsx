import {
  Building2,
  FileBarChart,
  KeyRound,
  LogOut,
  PencilLine,
  ScrollText,
  Users,
} from "lucide-react";
import { BrandMark } from "@/components/brand/Brand";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import type { HubPermission } from "./api";
import { LanguageSwitch } from "./LanguageSwitch";
import { useHubSession } from "./session";
import { ActivityPage } from "./pages/ActivityPage";
import { CorrectionsPage } from "./pages/CorrectionsPage";
import { EscrowPage } from "./pages/EscrowPage";
import { ReportsPage } from "./pages/ReportsPage";
import { SchoolsPage } from "./pages/SchoolsPage";
import { UsersPage } from "./pages/UsersPage";

interface Page {
  path: string;
  label: TranslationKey;
  icon: typeof Building2;
  /** Any one of these permissions opens the page. */
  needs: HubPermission[];
  element: React.ReactNode;
}

/** The pages in menu order. The hub checks the permission again on every call. */
const PAGES: Page[] = [
  {
    path: "/schools",
    label: "hub.nav.schools",
    icon: Building2,
    needs: ["nexus.admin", "nexus.reports"],
    element: <SchoolsPage />,
  },
  {
    path: "/reports",
    label: "hub.nav.reports",
    icon: FileBarChart,
    needs: ["nexus.reports"],
    element: <ReportsPage />,
  },
  {
    path: "/corrections",
    label: "hub.nav.corrections",
    icon: PencilLine,
    needs: ["nexus.admin"],
    element: <CorrectionsPage />,
  },
  {
    path: "/escrow",
    label: "hub.nav.escrow",
    icon: KeyRound,
    needs: ["nexus.admin", "nexus.escrow"],
    element: <EscrowPage />,
  },
  {
    path: "/users",
    label: "hub.nav.users",
    icon: Users,
    needs: ["nexus.admin"],
    element: <UsersPage />,
  },
  {
    path: "/activity",
    label: "hub.nav.activity",
    icon: ScrollText,
    needs: ["nexus.admin"],
    element: <ActivityPage />,
  },
];

export function Shell() {
  const { t } = useI18n();
  const { me, can, signOut } = useHubSession();
  const pages = PAGES.filter((p) => p.needs.some(can));

  return (
    <div className="flex h-screen bg-bg text-text">
      <aside
        aria-label={t("hub.name")}
        className="flex w-sidebar shrink-0 flex-col border-e border-border bg-sidebar"
      >
        <div className="flex h-topbar items-center gap-2 border-b border-border px-3">
          <BrandMark className="h-7 w-7" />
          <div className="min-w-0 leading-tight">
            <div
              className="nav-label truncate text-sm font-semibold text-navy"
              title={me.hub_name ?? undefined}
            >
              {me.hub_name ?? t("hub.name")}
            </div>
            <div className="nav-label truncate text-2xs uppercase tracking-wide text-muted">
              {t("hub.name")}
            </div>
          </div>
        </div>
        <nav aria-label={t("nav.modules")} className="flex-1 overflow-y-auto py-2">
          <ul className="flex flex-col gap-0.5 px-2">
            {pages.map(({ path, label, icon: Icon }) => (
              <li key={path}>
                <NavLink
                  to={path}
                  className={({ isActive }) =>
                    cn(
                      "flex h-8 items-center gap-2 rounded px-2 text-sm",
                      isActive
                        ? "bg-selection font-semibold text-blue"
                        : "text-text hover:bg-hover",
                    )
                  }
                >
                  <Icon className="h-4 w-4 shrink-0" aria-hidden />
                  <span className="nav-label truncate">{t(label)}</span>
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-topbar shrink-0 items-center gap-3 border-b border-border bg-card px-4">
          <div className="flex-1" />
          <LanguageSwitch />
          <div className="text-end leading-tight">
            <div className="text-sm font-medium text-text">{me.user.full_name}</div>
            <div className="text-2xs text-muted" dir="ltr">
              {me.user.username}
            </div>
          </div>
          <button
            type="button"
            onClick={signOut}
            aria-label={t("action.logout")}
            title={t("action.logout")}
            className="flex h-8 w-8 items-center justify-center rounded text-muted hover:bg-hover hover:text-text"
          >
            <LogOut className="h-4 w-4 rtl:-scale-x-100" aria-hidden />
          </button>
        </header>
        <main className="min-h-0 flex-1 overflow-y-auto p-4">
          {pages.length === 0 ? (
            <p role="status" className="text-sm text-muted">
              {t("hub.noAccess")}
            </p>
          ) : (
            <Routes>
              {pages.map((p) => (
                <Route key={p.path} path={p.path} element={p.element} />
              ))}
              <Route path="*" element={<Navigate to={pages[0]!.path} replace />} />
            </Routes>
          )}
        </main>
      </div>
    </div>
  );
}
