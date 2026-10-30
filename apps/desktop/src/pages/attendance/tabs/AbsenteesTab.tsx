import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "@/api/endpoints";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { StatusBadge, todayIso } from "../common";
import { ExportMenu } from "@/components/ExportMenu";

export function AbsenteesTab() {
  const { t } = useI18n();
  const [date, setDate] = useState(todayIso());
  const [classId, setClassId] = useState("");
  const classes = useAsync(() => api.classes(), []);
  const list = useAsync(
    () => api.absentees({ date, class_id: classId || undefined }),
    [date, classId],
  );
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <Input
          label={t("attendance.date")}
          type="date"
          value={date}
          max={todayIso()}
          onChange={(e) => e.target.value && setDate(e.target.value)}
          className="w-44"
        />
        <Select
          label={t("common.class")}
          value={classId}
          onChange={(e) => setClassId(e.target.value)}
          options={[
            { value: "", label: t("common.all") },
            ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
          ]}
          className="w-36"
        />
        {list.data && (
          <span className="pb-2 text-xs text-muted">
            {t("common.results", { count: list.data.length })}
          </span>
        )}
        <span className="flex-1" />
        <ExportMenu path="attendance/absentees" params={{ date, class_id: classId || undefined }} />
      </div>
      <Card padded={false}>
        {list.loading && !list.data ? (
          <div className="flex justify-center py-10">
            <Spinner />
          </div>
        ) : list.error ? (
          <EmptyState title={errorMessage(list.error, t("common.error.load"))} />
        ) : list.data!.length === 0 ? (
          <EmptyState title={t("attendance.absentees.empty")} />
        ) : (
          <table className="w-full text-sm">
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2 text-start">{t("students.fullName")}</th>
                <th className="px-3 py-2 text-start">{t("students.classSection")}</th>
                <th className="px-3 py-2 text-start">{t("common.status")}</th>
                <th className="px-3 py-2 text-start">{t("attendance.note")}</th>
                <th className="px-3 py-2 text-start">{t("attendance.absentees.phone")}</th>
              </tr>
            </thead>
            <tbody>
              {list.data!.map((r) => (
                <tr key={r.student_id} className="border-t border-border">
                  <td className="px-3 py-1.5">
                    <Link to={`/students/${r.student_id}`} className="font-medium hover:underline">
                      {r.full_name}
                    </Link>{" "}
                    <span className="text-xs text-muted">{r.student_number}</span>
                  </td>
                  <td className="px-3 py-1.5">
                    {r.class_name}-{r.section_name}
                  </td>
                  <td className="px-3 py-1.5">
                    <StatusBadge status={r.status} t={t} />
                  </td>
                  <td className="px-3 py-1.5 text-muted">{r.note ?? ""}</td>
                  <td className="px-3 py-1.5 tabular-nums">{r.guardian_phone ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}
