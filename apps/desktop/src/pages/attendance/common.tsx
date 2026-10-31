import type { AttendanceStatus, DayKind, StaffStatus } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import type { TFunction } from "@/i18n";

export const STATUSES: AttendanceStatus[] = ["present", "absent", "late", "leave"];
export const STAFF_STATUSES: StaffStatus[] = ["present", "absent", "leave"];

/** Keyboard letters for the marking screen (Latin letters work on Urdu keyboards too). */
export const STATUS_KEYS: Record<string, AttendanceStatus> = {
  p: "present",
  a: "absent",
  l: "late",
  v: "leave",
};

export const STATUS_TONES: Record<AttendanceStatus, BadgeTone> = {
  present: "success",
  absent: "danger",
  late: "warning",
  leave: "info",
};

export function statusLabel(t: TFunction, s: AttendanceStatus | StaffStatus) {
  return t(`attendance.status.${s}`);
}

export function StatusBadge({ status, t }: { status: AttendanceStatus; t: TFunction }) {
  return <Badge tone={STATUS_TONES[status]}>{statusLabel(t, status)}</Badge>;
}

/** Explains a weekend or holiday; renders nothing on a school day. */
export function DayKindNotice({ kind, t }: { kind: DayKind; t: TFunction }) {
  if (kind.kind === "school") return null;
  return (
    <p
      role="status"
      className="rounded border border-warning bg-warning-bg px-3 py-2 text-sm text-text"
    >
      {kind.kind === "weekend"
        ? t("attendance.day.weekend")
        : t("attendance.day.holiday", { name: kind.name })}
    </p>
  );
}

export { todayIso } from "@/lib/dates";

export function percentLabel(p: number | null | undefined): string {
  return p === null || p === undefined ? "—" : `${p.toFixed(1)}%`;
}
