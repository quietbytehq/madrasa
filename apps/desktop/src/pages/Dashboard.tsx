import { AlertTriangle, CalendarCheck, Coins, DatabaseBackup, Users } from "lucide-react";
import { Link } from "react-router-dom";
import { api } from "@/api/endpoints";
import { Badge } from "@/components/ui/Badge";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { useI18n } from "@/i18n";
import { formatMoney } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { useFeatures } from "@/app/features";
import { useSession } from "@/app/session";
import { PageHeader } from "./PageHeader";

export function Dashboard() {
  const { t, formatDate } = useI18n();
  const { user, can } = useSession();
  const { edition } = useFeatures();
  const dash = useAsync(() => api.dashboard(), []);
  const att = dash.data?.attendance ?? null;
  const dayKind = dash.data?.day_kind;
  const noSchool =
    dayKind && dayKind.kind !== "school"
      ? dayKind.kind === "weekend"
        ? t("dashboard.attendance.weekend")
        : dayKind.name
      : null;

  const backup = dash.data?.backup ?? null;
  const fees = dash.data?.fees ?? null;
  const cards: Array<{
    key: string;
    icon: typeof Users;
    label: string;
    value: string;
    note: string;
  }> = [
    {
      key: "students",
      icon: Users,
      label: t("dashboard.card.students"),
      value: dash.data?.students_active != null ? String(dash.data.students_active) : "—",
      note:
        dash.data?.students_active != null
          ? `${t("dashboard.students.active")}${dash.data.staff_active != null ? ` · ${dash.data.staff_active} ${t("dashboard.staff.active")}` : ""}`
          : t("dashboard.card.placeholder"),
    },
    {
      key: "fees",
      icon: Coins,
      label: t("dashboard.card.feesCollected"),
      value: fees ? formatMoney(fees.collected_this_month) : "—",
      note: fees
        ? t("dashboard.fees.sinceMonthStart", { date: formatDate(fees.month_start) })
        : t("dashboard.card.placeholder"),
    },
    {
      key: "defaulters",
      icon: AlertTriangle,
      label: t("dashboard.card.defaulters"),
      value: fees ? String(fees.defaulters) : "—",
      note: fees
        ? t("dashboard.fees.outstanding", { amount: formatMoney(fees.outstanding) })
        : t("dashboard.card.placeholder"),
    },
  ];

  return (
    <>
      <PageHeader
        title={t("dashboard.title")}
        actions={
          <Badge tone="lavender">
            {t("dashboard.edition")}: {t(`edition.${edition}`)}
          </Badge>
        }
      />
      <p className="mb-4 text-sm text-muted">
        {t("dashboard.welcome", { name: user?.full_name ?? "" })}
      </p>
      <div className="grid grid-cols-2 gap-3 xl:grid-cols-5">
        <Card padded={false} className="p-3" data-testid="attendance-card">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium uppercase tracking-wide text-muted">
              {t("dashboard.card.attendanceToday")}
            </span>
            <CalendarCheck className="h-4 w-4 text-muted" aria-hidden />
          </div>
          <div className="mt-2 text-2xl font-semibold tabular-nums text-navy">
            {att?.totals.percent !== null && att?.totals.percent !== undefined
              ? `${att.totals.percent.toFixed(1)}%`
              : "—"}
          </div>
          <p className="mt-1 text-xs text-muted">
            {noSchool
              ? t("dashboard.attendance.noSchool", { reason: noSchool })
              : att
                ? t("dashboard.attendance.marked", {
                    marked: att.totals.marked,
                    total: att.totals.sections,
                  })
                : t("dashboard.card.placeholder")}
          </p>
        </Card>
        <Card padded={false} className="p-3" data-testid="backup-card">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium uppercase tracking-wide text-muted">
              {t("dashboard.card.backup")}
            </span>
            <DatabaseBackup className="h-4 w-4 text-muted" aria-hidden />
          </div>
          <div
            className={`mt-2 text-2xl font-semibold tabular-nums ${backup?.stale ? "text-danger" : "text-navy"}`}
          >
            {backup?.last_backup_at ? formatDate(backup.last_backup_at) : "—"}
          </div>
          <p className={`mt-1 text-xs ${backup?.stale ? "text-danger" : "text-muted"}`}>
            {backup
              ? backup.stale
                ? t("backup.status.stale")
                : t("backup.status.ok")
              : t("dashboard.card.placeholder")}{" "}
            {can("backup.manage") && (
              <Link to="/backup" className="text-blue hover:underline">
                {t("dashboard.backup.open")}
              </Link>
            )}
          </p>
        </Card>
        {cards.map(({ key, icon: Icon, label, value, note }) => (
          <Card key={key} padded={false} className="p-3" data-testid={`${key}-card`}>
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium uppercase tracking-wide text-muted">
                {label}
              </span>
              <Icon className="h-4 w-4 text-muted" aria-hidden />
            </div>
            <div
              className={`mt-2 text-2xl font-semibold tabular-nums ${key === "defaulters" && value !== "—" && value !== "0" ? "text-danger" : "text-navy"}`}
            >
              {value}
            </div>
            <p className="mt-1 text-xs text-muted">{note}</p>
          </Card>
        ))}
      </div>
      {att && !noSchool && (
        <Card
          title={t("dashboard.attendance.unmarked")}
          className="mt-3"
          padded={false}
          actions={
            <Link to="/attendance" className="text-xs text-blue hover:underline">
              {t("dashboard.attendance.open")}
            </Link>
          }
        >
          {att.unmarked.length === 0 ? (
            <EmptyState title={t("dashboard.attendance.allMarked")} />
          ) : (
            <ul className="flex flex-wrap gap-2 p-3 text-sm">
              {att.unmarked.map((s) => (
                <li key={s.section_id}>
                  <Link
                    to={`/attendance?tab=mark&section=${s.section_id}`}
                    className="inline-flex items-center gap-1 rounded border border-border px-2 py-1 hover:bg-hover"
                  >
                    <span className="font-medium">
                      {s.class_name}-{s.section_name}
                    </span>
                    {s.class_teacher_name && (
                      <span className="text-xs text-muted">{s.class_teacher_name}</span>
                    )}
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}
      <Card title={t("dashboard.recentActivity")} className="mt-3">
        <EmptyState
          title={t("dashboard.recentActivity")}
          description={t("dashboard.recentActivityEmpty")}
        />
      </Card>
    </>
  );
}
