import type { PaymentMethod, VoucherStatus } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import type { TFunction } from "@/i18n";
import { useMoney } from "@/lib/useMoney";

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
