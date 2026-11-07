import { Link } from "react-router-dom";
import { api } from "@/api/endpoints";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { TimetableGrid } from "./TimetableGrid";

/**
 * The logged-in teacher's own week on the Dashboard (Standard, `timetable` feature). Hidden
 * when the login is not linked to a staff record or nothing is published.
 */
export function MyTimetableCard() {
  const { t, formatDate } = useI18n();
  const mine = useAsync(() => api.myTimetable().catch(() => null), []);
  const view = mine.data;
  if (!view || !view.timetable) return null;
  const me = view.lessons[0]?.teacher_ids[0] ?? "";
  return (
    <Card
      title={t("tt.mine")}
      className="mt-3"
      padded={false}
      data-testid="my-timetable"
      actions={
        <Link to="/timetable" className="text-xs text-blue hover:underline">
          {t("tt.title")}
        </Link>
      }
    >
      <div className="p-3">
        {view.lessons.length === 0 ? (
          <EmptyState title={t("tt.mine.empty")} />
        ) : (
          <TimetableGrid view={view} kind="teacher" id={me} today={view.today} compact />
        )}
        {view.covers.length > 0 && (
          <div className="mt-3">
            <div className="text-xs font-medium text-muted">{t("tt.mine.covers")}</div>
            <ul className="mt-1 flex flex-col gap-1 text-xs">
              {view.covers.map((c, i) => (
                <li key={i}>
                  <span className="font-medium">{formatDate(c.date)}</span> · {t("tt.period")}{" "}
                  {c.period}
                  {c.length > 1 && `–${c.period + c.length - 1}`} · {c.subject} ·{" "}
                  {c.sections.join(", ")} <span className="text-muted">({c.absent_teacher})</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Card>
  );
}
