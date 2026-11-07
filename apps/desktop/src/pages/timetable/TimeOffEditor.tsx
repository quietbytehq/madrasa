import type { TimeOffCell, TimetableShift } from "@/api/types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { dayLabel } from "./common";

/**
 * A day × period grid per shift; clicking a cell cycles available → avoid → never.
 * Used for teachers, sections and rooms.
 */
export function TimeOffEditor({
  shifts,
  value,
  onChange,
}: {
  shifts: TimetableShift[];
  value: TimeOffCell[];
  onChange: (cells: TimeOffCell[]) => void;
}) {
  const { t } = useI18n();
  const stateOf = (shift: string, day: number, period: number) =>
    value.find((c) => c.shift_id === shift && c.day === day && c.period === period)?.state ?? 0;
  const cycle = (shift: string, day: number, period: number) => {
    const next = (stateOf(shift, day, period) + 1) % 3;
    const rest = value.filter(
      (c) => !(c.shift_id === shift && c.day === day && c.period === period),
    );
    onChange(next === 0 ? rest : [...rest, { shift_id: shift, day, period, state: next }]);
  };
  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-muted">{t("tt.rules.timeOff.hint")}</p>
      {shifts
        .filter((s) => s.enabled)
        .map((s) => {
          const days = s.days.map((d, i) => (d.working ? i : -1)).filter((i) => i >= 0);
          const periods = Math.max(0, ...s.days.map((d) => (d.working ? d.periods : 0)));
          return (
            <div key={s.id} className="overflow-x-auto">
              {shifts.filter((x) => x.enabled).length > 1 && (
                <div className="mb-1 text-xs font-medium">{s.name}</div>
              )}
              <table className="border-collapse text-xs">
                <thead>
                  <tr>
                    <th className="px-1 text-start font-medium text-muted">{t("tt.period")}</th>
                    {days.map((d) => (
                      <th key={d} className="px-1 text-center font-medium text-muted">
                        {dayLabel(t, d, true)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {Array.from({ length: periods }, (_, i) => i + 1).map((p) => (
                    <tr key={p}>
                      <th className="px-1 text-start font-normal">{p}</th>
                      {days.map((d) => {
                        const off = (s.days[d]?.periods ?? 0) < p;
                        const state = stateOf(s.id, d, p);
                        return (
                          <td key={d} className="p-0.5">
                            <button
                              type="button"
                              disabled={off}
                              aria-label={`${dayLabel(t, d)} ${p}`}
                              data-state={state}
                              onClick={() => cycle(s.id, d, p)}
                              className={cn(
                                "h-7 w-12 rounded border text-[10px]",
                                off
                                  ? "bg-surface border-transparent"
                                  : state === 2
                                    ? "border-danger bg-danger/15 text-danger"
                                    : state === 1
                                      ? "border-warning bg-warning-bg text-warning"
                                      : "border-border bg-card hover:bg-selection",
                              )}
                            >
                              {state === 2
                                ? t("tt.rules.timeOff.never")
                                : state === 1
                                  ? t("tt.rules.timeOff.avoid")
                                  : ""}
                            </button>
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        })}
    </div>
  );
}
