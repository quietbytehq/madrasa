import { ChevronsLeft, ChevronsRight } from "lucide-react";
import { BrandMark } from "@/components/brand/Brand";
import { NavLink } from "react-router-dom";
import { cn } from "@/lib/cn";
import { useI18n } from "@/i18n";
import { useFeatures } from "@/app/features";
import { useSession } from "@/app/session";
import { visibleNavItems } from "@/app/navigation";
import { PARENT_APP_PERMISSIONS, useParentAppStatus } from "@/app/parentApp";
import { Badge } from "@/components/ui/Badge";

export interface SidebarProps {
  collapsed: boolean;
  onToggle: () => void;
}

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const { t } = useI18n();
  const { has, edition } = useFeatures();
  const { can } = useSession();
  // Only people with something to do in the Parent app ask whether the school is joined.
  const parentApp = useParentAppStatus(
    has("nexus_link") && PARENT_APP_PERMISSIONS.some((p) => can(p)),
  );
  const items = visibleNavItems(has, can, parentApp?.joined ?? false);
  const unread = parentApp?.unread ?? 0;

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
        <BrandMark className="h-7 w-7" />
        {!collapsed && (
          <div className="min-w-0 leading-tight">
            <div className="truncate font-display text-sm font-black text-navy">
              {t("app.name")}
            </div>
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
                  {item.path === "/parent-app" && unread > 0 && !collapsed && (
                    <Badge
                      tone="info"
                      className="ms-auto"
                      aria-label={t("pa.msg.unread", { count: unread })}
                      data-testid="nav-unread"
                    >
                      {unread}
                    </Badge>
                  )}
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
