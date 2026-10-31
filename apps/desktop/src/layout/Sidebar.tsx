import { ChevronsLeft, ChevronsRight } from "lucide-react";
import { NavLink } from "react-router-dom";
import { cn } from "@/lib/cn";
import { useI18n } from "@/i18n";
import { useFeatures } from "@/app/features";
import { useSession } from "@/app/session";
import { visibleNavItems } from "@/app/navigation";

export interface SidebarProps {
  collapsed: boolean;
  onToggle: () => void;
}

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const { t } = useI18n();
  const { has, edition } = useFeatures();
  const { can } = useSession();
  const items = visibleNavItems(has, can);

  return (
    <aside
      aria-label={t("app.name")}
      className={cn(
        "flex h-full shrink-0 flex-col border-e border-border bg-sidebar transition-[width]",
        collapsed ? "w-14" : "w-sidebar",
      )}
    >
      <div
        className={cn(
          "flex h-topbar items-center gap-2 border-b border-border px-3",
          collapsed && "justify-center px-0",
        )}
      >
        <div
          aria-hidden
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded bg-navy text-xs font-bold text-white"
        >
          M
        </div>
        {!collapsed && (
          <div className="min-w-0 leading-tight">
            <div className="truncate text-sm font-semibold text-navy">{t("app.name")}</div>
            <div className="truncate text-2xs uppercase tracking-wide text-muted">
              {t(`edition.${edition}`)}
            </div>
          </div>
        )}
      </div>
      <nav aria-label={t("nav.modules")} className="scrollbar-thin flex-1 overflow-y-auto py-2">
        <ul className="flex flex-col gap-0.5 px-2">
          {items.map((item) => {
            const Icon = item.icon;
            const label = t(item.labelKey);
            return (
              <li key={item.path}>
                <NavLink
                  to={item.path}
                  end={item.path === "/"}
                  title={collapsed ? label : undefined}
                  aria-label={collapsed ? label : undefined}
                  className={({ isActive }) =>
                    cn(
                      "flex h-8 items-center gap-2.5 rounded px-2 text-sm font-medium text-text/80 hover:bg-hover hover:text-text",
                      isActive && "bg-selection text-blue hover:bg-selection hover:text-blue",
                      collapsed && "justify-center px-0",
                    )
                  }
                >
                  <Icon className="h-4 w-4 shrink-0" aria-hidden />
                  {!collapsed && <span className="nav-label truncate">{label}</span>}
                </NavLink>
              </li>
            );
          })}
        </ul>
      </nav>
      <div className="border-t border-border p-2">
        <button
          type="button"
          onClick={onToggle}
          aria-label={collapsed ? t("nav.expand") : t("nav.collapse")}
          aria-expanded={!collapsed}
          className="flex h-8 w-full items-center justify-center rounded text-muted hover:bg-hover hover:text-text"
        >
          {collapsed ? (
            <ChevronsRight className="h-4 w-4 rtl:rotate-180" aria-hidden />
          ) : (
            <ChevronsLeft className="h-4 w-4 rtl:rotate-180" aria-hidden />
          )}
        </button>
      </div>
    </aside>
  );
}
