import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { StaffStatus } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useHasFeature } from "@/app/features";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { DayKindNotice, STAFF_STATUSES, statusLabel, todayIso } from "../common";

type Mark = { status: StaffStatus; note: string; arrived_at: string };

/** Simple daily present / absent / leave for staff. Admins save; others only view. */
export function StaffTab() {
  const { t } = useI18n();
  const has = useHasFeature();
  const toast = useToast();
  const lateAllowed = has("leave");
  const [date, setDate] = useState(todayIso());
  const day = useAsync(() => api.staffAttendance(date), [date]);
  const [marks, setMarks] = useState<Record<string, Mark>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!day.data) return;
    setMarks(
      Object.fromEntries(
        day.data.rows.map((r) => [
          r.staff_id,
          {
            status: r.status ?? "present",
            note: r.note ?? "",
            arrived_at: r.arrived_at?.slice(0, 5) ?? "",
          },
        ]),
      ),
    );
  }, [day.data]);
  const save = async () => {
    setBusy(true);
    try {
      const entries = Object.entries(marks)
        .filter(([, m]) => m.status !== "present" || m.note.trim())
        .map(([staff_id, m]) => ({
          staff_id,
          status: m.status,
          note: m.note.trim() || undefined,
          arrived_at: m.status === "late" && m.arrived_at ? m.arrived_at : undefined,
        }));
      day.setData(await api.saveStaffAttendance({ date, entries }));
      toast.push({ kind: "success", title: t("attendance.mark.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const v = day.data;
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
        <span className="flex-1" />
        {v?.can_edit && (
          <Button loading={busy} onClick={() => void save()}>
            {t("attendance.staff.save")}
          </Button>
        )}
      </div>
      {v && <DayKindNotice kind={v.day_kind} t={t} />}
      <Card padded={false}>
        {day.loading && !v ? (
          <div className="flex justify-center py-10">
            <Spinner />
          </div>
        ) : day.error || !v ? (
          <EmptyState title={errorMessage(day.error, t("common.error.load"))} />
        ) : v.rows.length === 0 ? (
          <EmptyState title={t("attendance.staff.empty")} />
        ) : (
          <table className="w-full text-sm">
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2 text-start">{t("students.fullName")}</th>
                <th className="px-3 py-2 text-start">{t("attendance.staff.designation")}</th>
                <th className="px-3 py-2 text-start">{t("common.status")}</th>
                <th className="px-3 py-2 text-start">{t("attendance.note")}</th>
              </tr>
            </thead>
            <tbody>
              {v.rows.map((r) => {
                const m = marks[r.staff_id] ?? {
                  status: "present" as const,
                  note: "",
                  arrived_at: "",
                };
                const locked = !!r.leave_request_id;
                return (
                  <tr key={r.staff_id} className="border-t border-border">
                    <td className="px-3 py-1">
                      <span className="font-medium">{r.full_name}</span>{" "}
                      <span className="text-xs text-muted">{r.staff_number}</span>
                    </td>
                    <td className="px-3 py-1 text-muted">{r.designation}</td>
                    <td className="px-3 py-1">
                      <select
                        aria-label={`${r.full_name}: ${t("common.status")}`}
                        value={m.status}
                        disabled={!v.can_edit || locked}
                        onChange={(e) =>
                          setMarks((all) => ({
                            ...all,
                            [r.staff_id]: { ...m, status: e.target.value as StaffStatus },
                          }))
                        }
                        className="h-7 rounded border border-border bg-card px-2 text-xs"
                      >
                        {STAFF_STATUSES.filter(
                          (s) => s !== "late" || lateAllowed || m.status === "late",
                        ).map((s) => (
                          <option key={s} value={s}>
                            {statusLabel(t, s)}
                          </option>
                        ))}
                      </select>
                      {lateAllowed && m.status === "late" && (
                        <input
                          type="time"
                          aria-label={`${r.full_name}: ${t("attendance.staff.arrivedAt")}`}
                          value={m.arrived_at}
                          disabled={!v.can_edit}
                          onChange={(e) =>
                            setMarks((all) => ({
                              ...all,
                              [r.staff_id]: { ...m, arrived_at: e.target.value },
                            }))
                          }
                          className="ms-1 h-7 rounded border border-border bg-card px-1 text-xs"
                        />
                      )}
                      {locked && (
                        <span className="ms-1 text-xs text-muted">
                          {t("attendance.staff.approvedLeave")}
                        </span>
                      )}
                    </td>
                    <td className="px-3 py-1">
                      <input
                        aria-label={`${r.full_name}: ${t("attendance.note")}`}
                        value={m.note}
                        disabled={!v.can_edit}
                        onChange={(e) =>
                          setMarks((all) => ({
                            ...all,
                            [r.staff_id]: { ...m, note: e.target.value },
                          }))
                        }
                        className="h-7 w-full rounded border border-border bg-card px-2 text-xs"
                      />
                    </td>
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
