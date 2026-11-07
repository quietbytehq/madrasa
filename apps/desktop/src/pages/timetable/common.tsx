import type { ReactNode } from "react";
import type { TimetableShift } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";

export const DAYS = [0, 1, 2, 3, 4, 5, 6] as const;

export function dayLabel(t: (k: TranslationKey) => string, day: number, short = false): string {
  return t(`tt.day.${short ? "short." : ""}${day}` as TranslationKey);
}

/** "Mon–Thu 7, Fri 5" style summary of a shift's week. */
export function shiftDaysSummary(
  t: (k: TranslationKey, p?: Record<string, string | number>) => string,
  shift: TimetableShift,
): string {
  return shift.days
    .map((d, i) => (d.working && d.periods > 0 ? `${dayLabel(t, i, true)} ${d.periods}` : null))
    .filter((x): x is string => x !== null)
    .join(", ");
}

/** Small hint paragraph above a card's content. */
export function Hint({ children }: { children: ReactNode }) {
  return <p className="mb-3 text-xs text-muted">{children}</p>;
}

export function StatusBadge({ status }: { status: "draft" | "published" }) {
  const { t } = useI18n();
  return (
    <Badge tone={status === "published" ? "success" : "neutral"}>
      {t(status === "published" ? "tt.timetable.published" : "tt.timetable.draft")}
    </Badge>
  );
}

/** A checkbox list for picking several ids. */
export function CheckList({
  label,
  items,
  value,
  onChange,
  columns = 2,
}: {
  label: ReactNode;
  items: { id: string; name: string }[];
  value: string[];
  onChange: (ids: string[]) => void;
  columns?: number;
}) {
  return (
    <fieldset className="min-w-0">
      <legend className="mb-1 text-xs font-medium text-muted">{label}</legend>
      <div
        className="grid max-h-48 gap-x-3 gap-y-1 overflow-y-auto rounded border border-border p-2 text-sm"
        style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
      >
        {items.map((it) => (
          <label key={it.id} className="flex items-center gap-2 truncate">
            <input
              type="checkbox"
              checked={value.includes(it.id)}
              onChange={(e) =>
                onChange(e.target.checked ? [...value, it.id] : value.filter((v) => v !== it.id))
              }
            />
            <span className="truncate">{it.name}</span>
          </label>
        ))}
        {items.length === 0 && <span className="text-xs text-muted">—</span>}
      </div>
    </fieldset>
  );
}

export function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
