import { Badge } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";
import type { CapacityWarning } from "@/api/types";

/** Seats left or the over-capacity warning of a route. */
export function CapacityBadge({
  warning,
  seatsLeft,
}: {
  warning: CapacityWarning | null;
  seatsLeft: number | null;
}) {
  const { t } = useI18n();
  if (warning?.kind === "over") {
    return <Badge tone="danger">{t("trp.capacity.over", { n: warning.by })}</Badge>;
  }
  if (warning?.kind === "full") {
    return <Badge tone="warning">{t("trp.capacity.full")}</Badge>;
  }
  if (seatsLeft === null) return null;
  return <Badge tone="success">{t("trp.capacity.left", { n: seatsLeft })}</Badge>;
}

export function time(v: string | null): string {
  return v ? v.slice(0, 5) : "";
}
