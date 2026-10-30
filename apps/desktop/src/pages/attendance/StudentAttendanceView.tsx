import { api } from "@/api/endpoints";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { percentLabel, StatusBadge } from "./common";

/** Attendance tab on the student profile: summary since the start of the year and the days. */
export function StudentAttendanceView({ studentId }: { studentId: string }) {
  const { t, formatDate } = useI18n();
  const view = useAsync(() => api.studentAttendance(studentId), [studentId]);
  if (view.loading && !view.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (view.error || !view.data)
    return <EmptyState title={errorMessage(view.error, t("common.error.load"))} />;
  const v = view.data;
  const total = v.summary.present + v.summary.absent + v.summary.late + v.summary.leave;
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
        <Card padded={false} className="p-3">
          <div className="text-2xs uppercase tracking-wide text-muted">
            {t("attendance.percent")}
          </div>
          <div
            className={`mt-1 text-lg font-semibold tabular-nums ${v.percent !== null && v.percent < 75 ? "text-danger" : "text-navy"}`}
          >
            {percentLabel(v.percent)}
          </div>
          <div className="text-xs text-muted">{t("attendance.daysMarked", { count: total })}</div>
        </Card>
        {(["present", "absent", "late", "leave"] as const).map((k) => (
          <Card key={k} padded={false} className="p-3">
            <div className="text-2xs uppercase tracking-wide text-muted">
              {t(`attendance.status.${k}`)}
            </div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-navy">{v.summary[k]}</div>
          </Card>
        ))}
      </div>
      <Card title={t("attendance.profile.range")} padded={false}>
        {v.days.length === 0 ? (
          <EmptyState title={t("attendance.profile.empty")} />
        ) : (
          <ul className="max-h-96 divide-y divide-border overflow-y-auto text-sm">
            {v.days.map((d) => (
              <li key={d.date} className="flex items-center gap-3 px-4 py-1.5">
                <span className="w-28 text-muted">{formatDate(d.date)}</span>
                <StatusBadge status={d.status} t={t} />
                <span className="text-muted">{d.note ?? ""}</span>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}
