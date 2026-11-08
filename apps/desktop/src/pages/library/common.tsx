import type { CopyStatus, LoanView } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";

export const STATUS_TONES: Record<CopyStatus, BadgeTone> = {
  available: "success",
  on_loan: "info",
  lost: "danger",
  withdrawn: "neutral",
};

export function CopyStatusBadge({ status }: { status: CopyStatus }) {
  const { t } = useI18n();
  return <Badge tone={STATUS_TONES[status]}>{t(`lib.copy.status.${status}`)}</Badge>;
}

export function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function isOverdue(l: LoanView, now = today()): boolean {
  return l.returned_on === null && l.due_on < now;
}

/** Fine still owed on a loan (fine − waived, zero once settled or posted). */
export function fineDue(l: LoanView): number {
  if (l.fine_settled_on || l.fine_voucher_id) return 0;
  return Math.max(0, Number(l.fine) - Number(l.fine_waived));
}
