import { Lock } from "lucide-react";
import type { TimetableView } from "@/api/types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import type { TranslationKey } from "@/i18n";
import { buildGrid } from "./grid";
import type { GridCell, GridKind } from "./grid";

export interface TimetableGridProps {
  view: TimetableView;
  kind: GridKind;
  id: string;
  /** Called when a placed session is clicked (edit mode). */
  onCellClick?: (cell: GridCell) => void;
  /** Highlight this weekday (0 = Monday). */
  today?: number | null;
  compact?: boolean;
}

export function dayKey(day: number, short = false): TranslationKey {
  return `tt.day.${short ? "short." : ""}${day}` as TranslationKey;
}

/** The weekly grid of one section, teacher or room. */
export function TimetableGrid({ view, kind, id, onCellClick, today, compact }: TimetableGridProps) {
  const { t } = useI18n();
  const blocks = buildGrid(view, kind, id);
  if (blocks.length === 0) return null;
  const many = blocks.length > 1;
  return (
    <div className="flex flex-col gap-3" data-testid="timetable-grid">
      {blocks.map((b) => (
        <div key={b.shift.id} className="overflow-x-auto">
          {many && <div className="mb-1 text-xs font-medium text-muted">{b.shift.name}</div>}
          <table className="w-full table-fixed border-collapse text-xs">
            <thead>
              <tr>
                <th className="bg-surface w-16 border border-border px-1 py-1 text-start font-medium text-muted">
                  {t("tt.period")}
                </th>
                {b.days.map((d) => (
                  <th
                    key={d}
                    className={cn(
                      "bg-surface border border-border px-1 py-1 text-center font-medium",
                      today === d ? "text-blue" : "text-muted",
                    )}
                  >
                    {t(dayKey(d, !!compact))}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {b.rows.map((row) => (
                <tr
                  key={row.period}
                  className={cn(b.breaksAfter.includes(row.period) && "border-b-4 border-b-border")}
                >
                  <th className="bg-surface border border-border px-1 py-1 text-start font-normal">
                    <div className="font-medium">{row.period}</div>
                    {!compact && <div className="text-[10px] text-muted">{row.time}</div>}
                  </th>
                  {row.cells.map((cell, i) => {
                    const day = b.days[i] ?? 0;
                    const off = (b.shift.days[day]?.periods ?? 0) < row.period;
                    if (!cell) {
                      return (
                        <td
                          key={day}
                          className={cn(
                            "border border-border px-1 py-1 align-top",
                            off ? "bg-surface" : "",
                            today === day && !off ? "bg-selection/40" : "",
                          )}
                        >
                          {!off && compact && (
                            <span className="text-muted">{t("tt.mine.free")}</span>
                          )}
                        </td>
                      );
                    }
                    const clickable = !!onCellClick;
                    return (
                      <td
                        key={day}
                        className={cn(
                          "border border-border p-0 align-top",
                          cell.continued ? "border-t-dashed" : "",
                        )}
                      >
                        <button
                          type="button"
                          disabled={!clickable}
                          onClick={() => onCellClick?.(cell)}
                          data-testid={`cell-${cell.card.lesson_id}-${cell.card.number}`}
                          className={cn(
                            "block h-full w-full px-1 py-1 text-start",
                            clickable
                              ? "hover:bg-selection focus-visible:bg-selection"
                              : "cursor-default",
                            cell.continued ? "text-muted" : "",
                          )}
                        >
                          <div className="flex items-center gap-1 font-medium text-navy">
                            <span className="truncate">{cell.title}</span>
                            {cell.card.locked && !cell.continued && (
                              <Lock
                                className="h-3 w-3 shrink-0 text-muted"
                                aria-label={t("tt.view.locked")}
                              />
                            )}
                          </div>
                          {!cell.continued && (
                            <div className="truncate text-muted">
                              {cell.detail}
                              {cell.room && ` · ${cell.room}`}
                            </div>
                          )}
                        </button>
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
    </div>
  );
}
