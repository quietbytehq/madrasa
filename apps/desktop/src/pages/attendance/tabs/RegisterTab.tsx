import { useEffect, useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { monthLabel, monthStart } from "@/lib/money";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { percentLabel } from "../common";

/** Students × days for one section and month; weekends and holidays are shaded. */
export function RegisterTab() {
  const { t } = useI18n();
  const [month, setMonth] = useState(monthStart());
  const [sectionId, setSectionId] = useState("");
  const status = useAsync(() => api.attendanceStatus(), []);
  const sections = useMemo(() => status.data?.sections ?? [], [status.data]);
  useEffect(() => {
    if (!sectionId && sections.length > 0) setSectionId(sections[0]!.section_id);
  }, [sections, sectionId]);
  const register = useAsync(
    () => (sectionId ? api.attendanceRegister(sectionId, month) : Promise.resolve(null)),
    [sectionId, month],
  );
  const r = register.data;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <Select
          label={t("attendance.section")}
          value={sectionId}
          onChange={(e) => setSectionId(e.target.value)}
          options={sections.map((s) => ({
            value: s.section_id,
            label: `${s.class_name}-${s.section_name}`,
          }))}
          className="w-48"
        />
        <Input
          label={t("attendance.month")}
          type="month"
          value={month.slice(0, 7)}
          onChange={(e) => e.target.value && setMonth(`${e.target.value}-01`)}
          className="w-44"
        />
        {r && (
          <span className="pb-2 text-xs text-muted">
            {monthLabel(r.month)} · {t("attendance.register.schoolDays", { count: r.school_days })}
          </span>
        )}
      </div>
      <Card padded={false} className="overflow-x-auto">
        {register.loading && !r ? (
          <div className="flex justify-center py-10">
            <Spinner />
          </div>
        ) : register.error ? (
          <EmptyState title={errorMessage(register.error, t("common.error.load"))} />
        ) : !r || r.rows.length === 0 ? (
          <EmptyState title={t("attendance.mark.noStudents")} />
        ) : (
          <table className="text-xs">
            <thead className="bg-hover text-muted">
              <tr>
                <th className="sticky start-0 bg-hover px-2 py-1 text-start">
                  {t("attendance.register.student")}
                </th>
                {r.days.map((d) => (
                  <th
                    key={d.date}
                    title={
                      d.kind.kind === "holiday"
                        ? d.kind.name
                        : d.kind.kind === "weekend"
                          ? t("weekday.7")
                          : d.marked
                            ? ""
                            : t("attendance.register.notMarked")
                    }
                    className={cn(
                      "w-6 px-0.5 py-1 text-center font-normal tabular-nums",
                      d.kind.kind !== "school" && "bg-lavender/60",
                    )}
                  >
                    {Number(d.date.slice(8, 10))}
                  </th>
                ))}
                {(["present", "absent", "late", "leave"] as const).map((k) => (
                  <th key={k} className="px-1.5 py-1 text-center">
                    {t(`attendance.short.${k}`)}
                  </th>
                ))}
                <th className="px-1.5 py-1 text-center">{t("attendance.register.percent")}</th>
              </tr>
            </thead>
            <tbody>
              {r.rows.map((row) => {
                const total =
                  row.summary.present + row.summary.absent + row.summary.late + row.summary.leave;
                const pct =
                  total === 0
                    ? null
                    : Math.round(((row.summary.present + row.summary.late) * 1000) / total) / 10;
                return (
                  <tr key={row.student_id} className="border-t border-border">
                    <td className="sticky start-0 whitespace-nowrap bg-card px-2 py-1">
                      <span className="text-muted">{row.roll_no ?? ""}</span> {row.full_name}
                    </td>
                    {row.statuses.map((s, i) => (
                      <td
                        key={i}
                        className={cn(
                          "px-0.5 py-1 text-center",
                          r.days[i]!.kind.kind !== "school" && "bg-lavender/60",
                          s === "absent" && "font-semibold text-danger",
                          s === "late" && "text-warning",
                          s === "leave" && "text-blue",
                          s === "present" && "text-success",
                        )}
                      >
                        {s ? t(`attendance.short.${s}`) : ""}
                      </td>
                    ))}
                    <td className="px-1.5 py-1 text-center tabular-nums">{row.summary.present}</td>
                    <td className="px-1.5 py-1 text-center tabular-nums">{row.summary.absent}</td>
                    <td className="px-1.5 py-1 text-center tabular-nums">{row.summary.late}</td>
                    <td className="px-1.5 py-1 text-center tabular-nums">{row.summary.leave}</td>
                    <td className="px-1.5 py-1 text-center tabular-nums">{percentLabel(pct)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}
