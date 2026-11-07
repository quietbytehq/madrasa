import type { LeaveRequestStatus } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";

export const STATUS_TONES: Record<LeaveRequestStatus, BadgeTone> = {
  pending: "warning",
  approved: "success",
  rejected: "danger",
  withdrawn: "neutral",
};

export function StatusBadge({ status }: { status: LeaveRequestStatus }) {
  const { t } = useI18n();
  return <Badge tone={STATUS_TONES[status]}>{t(`leave.status.${status}`)}</Badge>;
}

/** "3", "0.5": decimal day counts without trailing zeros. */
export function days(v: string | number): string {
  const n = typeof v === "number" ? v : Number(v);
  return Number.isFinite(n) ? String(n) : String(v);
}

export function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function thisMonth(): string {
  return today().slice(0, 7);
}
