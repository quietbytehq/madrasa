import type { StageKind } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";

export const KIND_TONES: Record<StageKind, BadgeTone> = {
  open: "info",
  admitted: "success",
  rejected: "danger",
  withdrawn: "neutral",
};

export function KindBadge({ kind }: { kind: StageKind }) {
  const { t } = useI18n();
  return <Badge tone={KIND_TONES[kind]}>{t(`adm.kind.${kind}`)}</Badge>;
}

export function StageBadge({ name, kind }: { name: string; kind: StageKind }) {
  return <Badge tone={KIND_TONES[kind]}>{name}</Badge>;
}

export function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
