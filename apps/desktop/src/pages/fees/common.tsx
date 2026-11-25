import type { PaymentMethod, VoucherStatus } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import type { TFunction } from "@/i18n";
import { useI18n } from "@/i18n";
import { useMoney } from "@/lib/useMoney";
import { useWaitingPayments } from "@/lib/waitingPayments";

export const METHODS: PaymentMethod[] = ["cash", "bank", "online", "one_bill", "kuickpay"];

export function methodLabel(t: TFunction, m: PaymentMethod) {
  return t(`fees.method.${m}`);
}

const tones: Record<VoucherStatus, BadgeTone> = {
  unpaid: "danger",
  partial: "warning",
  paid: "success",
  carried: "neutral",
  cancelled: "neutral",
};

export function VoucherStatusBadge({ status, t }: { status: VoucherStatus; t: TFunction }) {
  return <Badge tone={tones[status]}>{t(`fees.status.${status}`)}</Badge>;
}

export function Amount({
  value,
  className,
}: {
  value: string | null | undefined;
  className?: string;
}) {
  const money = useMoney();
  return <span className={`tabular-nums ${className ?? ""}`}>{money(value)}</span>;
}

/**
 * "Paid (provisional, waiting to send): Rs 5,000" beside a voucher that has a payment taken
 * on this PC while the school server was out of reach. Nothing when there is none.
 */
export function WaitingPaid({ voucherId }: { voucherId: string }) {
  const { t } = useI18n();
  const money = useMoney();
  const waiting = useWaitingPayments().get(voucherId);
  if (!waiting) return null;
  return (
    <Badge tone="warning" className="ms-1 whitespace-normal" data-testid="waiting-paid">
      {t("fees.provisional.waiting", { amount: money(waiting.amount.toFixed(2)) })}
    </Badge>
  );
}
