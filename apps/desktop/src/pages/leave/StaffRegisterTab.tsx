import { useState } from "react";
import { api } from "@/api/endpoints";
import type { StaffRegisterCell } from "@/api/types";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { useAsync } from "@/lib/useAsync";
import { thisMonth } from "./common";

const TONE: Record<string, string> = {
  present: "text-success",
  absent: "text-danger font-medium",
  late: "text-warning font-medium",
  leave: "text-blue",
};

/** The monthly staff register: one row per staff member, a cell per day. */
export function StaffRegisterTab() {
  const { t } = useI18n();
  const [month, setMonth] = useState(thisMonth());
  const view = useAsync(() => api.staffRegister(month), [month]);
  const v = view.data;
  const code = (c: StaffRegisterCell) => t(`leave.register.code.${c.status}`);
  return (
    <Card
      actions={
        <Input
          label={t("leave.register.month")}
          type="month"
          value={month}
          onChange={(e) => e.target.value && setMonth(e.target.value)}
          className="w-40"
        />
      }
    >
      <p className="mb-3 text-xs text-muted">{t("leave.register.hint")}</p>
      {view.loading && !v ? (
        <div className="flex justify-center py-6">
          <Spinner />
        </div>
      ) : !v || v.rows.length === 0 ? (
        <EmptyState title={t("leave.register.empty")} />
      ) : (
        <div className="overflow-x-auto">
          <table className="border-collapse text-xs" data-testid="staff-register">
            <thead>
              <tr>
                <th className="sticky start-0 bg-card px-2 py-1 text-start font-medium text-muted">
                  {t("leave.request.staff")}
                </th>
                {v.days.map((d, i) => (
                  <th
                    key={d}
                    className={cn(
                      "w-7 px-0.5 py-1 text-center font-normal text-muted",
                      !v.school_days[i] && "bg-surface",
                    )}
                  >
                    {Number(d.slice(8))}
                  </th>
                ))}
                {(["present", "absent", "late", "leave"] as const).map((k) => (
                  <th key={k} className="px-1 py-1 text-end font-medium text-muted">
                    {t(`leave.register.code.${k}`)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {v.rows.map((r) => (
                <tr key={r.staff_id} className="border-t border-border">
                  <td className="sticky start-0 whitespace-nowrap bg-card px-2 py-1">
                    <span className="font-medium">{r.full_name}</span>{" "}
                    <span className="text-muted">{r.staff_number}</span>
                  </td>
                  {r.cells.map((c, i) => (
                    <td
                      key={i}
                      className={cn(
                        "px-0.5 py-1 text-center",
                        !v.school_days[i] && "bg-surface",
                        c && TONE[c.status],
                      )}
                      title={
                        c
                          ? `${v.days[i]}: ${c.status}${c.arrived_at ? ` ${c.arrived_at.slice(0, 5)}` : ""}${c.approved_leave ? " ✓" : ""}`
                          : undefined
                      }
                    >
                      {c ? (
                        <span>
                          {code(c)}
                          {c.approved_leave && <span className="text-[9px]">✓</span>}
                          {c.arrived_at && (
                            <span className="block text-[9px] text-muted">
                              {c.arrived_at.slice(0, 5)}
                            </span>
                          )}
                        </span>
                      ) : (
                        ""
                      )}
                    </td>
                  ))}
                  <td className="px-1 py-1 text-end tabular-nums">{r.present}</td>
                  <td className="px-1 py-1 text-end tabular-nums">{r.absent}</td>
                  <td className="px-1 py-1 text-end tabular-nums">{r.late}</td>
                  <td className="px-1 py-1 text-end tabular-nums">{r.leave}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}
