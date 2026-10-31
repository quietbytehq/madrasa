import { todayIso } from "@/lib/dates";
import { useState } from "react";
import { ExportMenu } from "@/components/ExportMenu";
import { Link } from "react-router-dom";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { Amount } from "../common";

export function DefaultersTab() {
  const { t, formatDate } = useI18n();
  const [asOf, setAsOf] = useState(todayIso());
  const [classId, setClassId] = useState("");
  const [minDays, setMinDays] = useState("0");
  const classes = useAsync(() => api.classes(), []);
  const list = useAsync(
    () =>
      api.defaulters({
        as_of: asOf,
        class_id: classId || undefined,
        min_days: Number(minDays) || 0,
      }),
    [asOf, classId, minDays],
  );
  const total = (list.data ?? []).reduce((a, d) => a + Number(d.balance), 0);
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2 print:hidden">
        <Input
          label={t("fees.defaulters.asOf")}
          type="date"
          value={asOf}
          onChange={(e) => setAsOf(e.target.value)}
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
        <Input
          label={t("fees.defaulters.minDays")}
          type="number"
          min={0}
          value={minDays}
          onChange={(e) => setMinDays(e.target.value)}
          className="w-36"
        />
        <span className="flex-1" />
        <ExportMenu
          path="fees/defaulters"
          params={{ as_of: asOf, class_id: classId || undefined, min_days: minDays || undefined }}
        />
        <Button variant="secondary" onClick={() => window.print()}>
          {t("action.print")}
        </Button>
      </div>
      <Card padded={false} className="print-area">
        {list.loading && !list.data ? (
          <div className="flex justify-center py-10">
            <Spinner />
          </div>
        ) : (list.data?.length ?? 0) === 0 ? (
          <EmptyState title={t("fees.defaulters.empty")} />
        ) : (
          <table className="w-full text-sm">
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2 text-start">{t("students.fullName")}</th>
                <th className="px-3 py-2 text-start">{t("students.fatherName")}</th>
                <th className="px-3 py-2 text-start">{t("students.classSection")}</th>
                <th className="px-3 py-2 text-start">{t("common.phone")}</th>
                <th className="px-3 py-2 text-start">{t("fees.dueDate")}</th>
                <th className="px-3 py-2 text-end">{t("fees.defaulters.days")}</th>
                <th className="px-3 py-2 text-end">{t("fees.defaulters.months")}</th>
                <th className="px-3 py-2 text-end">{t("fees.balance")}</th>
              </tr>
            </thead>
            <tbody>
              {list.data!.map((d) => (
                <tr key={d.student_id} className="border-t border-border">
                  <td className="px-3 py-1.5">
                    <Link
                      to={`/students/${d.student_id}`}
                      className="font-medium text-text hover:underline"
                    >
                      {d.student_name}
                    </Link>{" "}
                    <span className="text-xs text-muted">{d.student_number}</span>
                  </td>
                  <td className="px-3 py-1.5">{d.father_name}</td>
                  <td className="px-3 py-1.5">
                    {d.class_name ? `${d.class_name}-${d.section_name ?? ""}` : "—"}
                  </td>
                  <td className="px-3 py-1.5">{d.guardian_phone ?? "—"}</td>
                  <td className="px-3 py-1.5">{formatDate(d.oldest_due_date)}</td>
                  <td className="px-3 py-1.5 text-end tabular-nums">{d.days_overdue}</td>
                  <td className="px-3 py-1.5 text-end tabular-nums">{d.unpaid_vouchers}</td>
                  <td className="px-3 py-1.5 text-end font-medium text-danger">
                    <Amount value={d.balance} />
                  </td>
                </tr>
              ))}
            </tbody>
            <tfoot className="bg-hover text-xs font-semibold text-text">
              <tr>
                <td className="px-3 py-2" colSpan={7}>
                  {t("common.results", { count: list.data!.length })}
                </td>
                <td className="px-3 py-2 text-end">
                  <Amount value={total.toFixed(2)} />
                </td>
              </tr>
            </tfoot>
          </table>
        )}
      </Card>
    </div>
  );
}
