import { useMemo } from "react";
import {
  Bell,
  CalendarCheck,
  ChevronRight,
  GraduationCap,
  Home,
  MessageSquare,
  Wallet,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Link, NavLink, Outlet, useOutletContext, useParams } from "react-router-dom";
import { Badge } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import type { AsyncState } from "@/lib/useAsync";
import { hubApi } from "@parent/api";
import type { ChildRef, Overview } from "@parent/api";
import { percent } from "@parent/format";
import { LastUpdated, Loadable, Panel, Screen, useLtr } from "@parent/ui";

export interface ChildContext {
  child: ChildRef;
  /** The child's overview: name, class, and the counts the tiles and the bar show. */
  overview: AsyncState<Overview>;
}

export function useChild(): ChildContext {
  return useOutletContext<ChildContext>();
}

const SECTIONS: { path: string; labelKey: TranslationKey; icon: LucideIcon }[] = [
  { path: "", labelKey: "parent.nav.overview", icon: Home },
  { path: "attendance", labelKey: "parent.nav.attendance", icon: CalendarCheck },
  { path: "results", labelKey: "parent.nav.results", icon: GraduationCap },
  { path: "fees", labelKey: "parent.nav.fees", icon: Wallet },
  { path: "notices", labelKey: "parent.nav.notices", icon: Bell },
  { path: "messages", labelKey: "parent.nav.messages", icon: MessageSquare },
];

/** One child: the name on top, the six sections at the bottom, the chosen one between. */
export function ChildLayout() {
  const { t } = useI18n();
  const { school = "", student = "" } = useParams();
  const child = useMemo<ChildRef>(() => ({ school, student }), [school, student]);
  const overview = useAsync(() => hubApi.overview(child), [school, student]);
  const ltr = useLtr();
  const o = overview.data;
  const base = `/child/${school}/${student}`;
  return (
    <Screen
      title={o?.full_name ?? t("parent.child.loading")}
      subtitle={
        o && (
          <>
            {o.class_name && (
              <>
                <bdi>
                  {t("parent.child.class", {
                    class: ltr(o.section_name ? `${o.class_name}-${o.section_name}` : o.class_name),
                  })}
                </bdi>
                {" · "}
              </>
            )}
            <bdi>{o.school_name}</bdi>
          </>
        )
      }
      back="/"
      footer={
        <nav
          aria-label={t("parent.nav.label")}
          className="safe-bottom shrink-0 border-t border-border bg-card"
        >
          <ul className="grid grid-cols-6">
            {SECTIONS.map(({ path, labelKey, icon: Icon }) => (
              <li key={path}>
                <NavLink
                  to={path ? `${base}/${path}` : base}
                  end={path === ""}
                  className={({ isActive }) =>
                    cn(
                      "relative flex min-h-14 flex-col items-center justify-center gap-0.5 px-0.5 text-2xs",
                      isActive ? "font-semibold text-blue" : "text-muted hover:text-text",
                    )
                  }
                >
                  <Icon className="h-5 w-5" aria-hidden />
                  <span className="nav-label max-w-full truncate">{t(labelKey)}</span>
                  {path === "messages" && !!o && o.unread > 0 && (
                    <span
                      data-testid="nav-unread"
                      aria-label={t("parent.messages.unread", { count: o.unread })}
                      className="absolute end-2 top-1.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-blue px-1 text-[0.625rem] font-semibold text-white"
                    >
                      {o.unread}
                    </span>
                  )}
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
      }
    >
      <Outlet context={{ child, overview } satisfies ChildContext} />
    </Screen>
  );
}

function Tile({
  to,
  icon: Icon,
  label,
  value,
  note,
  testId,
}: {
  to: string;
  icon: LucideIcon;
  label: string;
  value: string;
  note?: string;
  testId: string;
}) {
  return (
    <Link
      to={to}
      data-testid={testId}
      className="flex items-center gap-3 rounded-lg border border-border bg-card p-3 shadow-card hover:bg-hover"
    >
      <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded bg-selection text-blue">
        <Icon className="h-5 w-5" aria-hidden />
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-xs text-muted">{label}</div>
        <div className="truncate text-base font-semibold text-text">{value}</div>
        {note && <div className="truncate text-xs text-muted">{note}</div>}
      </div>
      <ChevronRight className="h-4 w-4 shrink-0 text-muted rtl:rotate-180" aria-hidden />
    </Link>
  );
}

/** The child at a glance, each line leading to its section. */
export function OverviewScreen() {
  const { t, locale } = useI18n();
  const { overview } = useChild();
  const intl = locale === "ur" ? "ur-PK" : "en-PK";
  return (
    <Loadable state={overview}>
      {(o) => (
        <div className="flex flex-col gap-2">
          <Panel>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
              <dt className="text-muted">{t("parent.child.school")}</dt>
              <dd className="text-text">{o.school_name}</dd>
              <dt className="text-muted">{t("parent.child.classLabel")}</dt>
              <dd className="text-text">
                <bdi>
                  {o.class_name
                    ? o.section_name
                      ? `${o.class_name}-${o.section_name}`
                      : o.class_name
                    : t("parent.child.noClass")}
                </bdi>
                {o.roll_no !== null && (
                  <>
                    {" · "}
                    <bdi>{t("parent.child.roll", { no: o.roll_no })}</bdi>
                  </>
                )}
              </dd>
              <dt className="text-muted">{t("parent.child.teacher")}</dt>
              <dd className="text-text">{o.class_teacher ?? "—"}</dd>
              <dt className="text-muted">{t("parent.child.number")}</dt>
              <dd className="text-text" dir="ltr">
                <span className="block text-start">{o.student_number}</span>
              </dd>
            </dl>
            {o.status !== "active" && (
              <Badge tone="warning" className="mt-2">
                {t("parent.child.left")}
              </Badge>
            )}
          </Panel>
          <Tile
            to="attendance"
            icon={CalendarCheck}
            label={t("parent.nav.attendance")}
            value={
              o.attendance && o.attendance.total > 0
                ? percent(o.attendance.percent)
                : t("parent.attendance.none.short")
            }
            note={
              o.attendance && o.attendance.total > 0
                ? t("parent.attendance.ofDays", {
                    present: o.attendance.present + o.attendance.late,
                    total: o.attendance.total,
                  })
                : undefined
            }
            testId="tile-attendance"
          />
          <Tile
            to="results"
            icon={GraduationCap}
            label={t("parent.nav.results")}
            value={t("parent.results.open")}
            note={o.academic_year ?? undefined}
            testId="tile-results"
          />
          <Tile
            to="fees"
            icon={Wallet}
            label={t("parent.fees.due")}
            value={`${o.currency} ${formatMoney(o.balance, intl)}`}
            note={
              o.open_vouchers === 1
                ? t("parent.fees.openVoucher")
                : o.open_vouchers > 1
                  ? t("parent.fees.openVouchers", { count: o.open_vouchers })
                  : t("parent.fees.nothingDue")
            }
            testId="tile-fees"
          />
          <Tile
            to="notices"
            icon={Bell}
            label={t("parent.nav.notices")}
            value={t("parent.notices.count", { count: o.notices })}
            testId="tile-notices"
          />
          <Tile
            to="messages"
            icon={MessageSquare}
            label={t("parent.nav.messages")}
            value={
              o.unread > 0
                ? t("parent.messages.unread", { count: o.unread })
                : t("parent.messages.none.short")
            }
            note={
              o.class_teacher ? t("parent.messages.with", { name: o.class_teacher }) : undefined
            }
            testId="tile-messages"
          />
          <LastUpdated at={o.synced_at} />
        </div>
      )}
    </Loadable>
  );
}
