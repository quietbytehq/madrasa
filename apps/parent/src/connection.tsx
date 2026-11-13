import type { OutboxLink } from "@/api/types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { useHubLink } from "@parent/hubLink";

const DOT: Record<OutboxLink, string> = {
  connected: "bg-success",
  reconnecting: "bg-warning",
  offline: "bg-danger",
};

export function HubBadge() {
  const { t } = useI18n();
  const link = useHubLink();
  return (
    <span
      role="status"
      className="flex shrink-0 items-center gap-1 rounded border border-border px-1.5 py-0.5 text-2xs text-text"
      data-testid="hub-link"
      title={t(`parent.link.explain.${link}`)}
    >
      <span className={cn("h-2 w-2 rounded-full", DOT[link])} aria-hidden />
      {t(`offline.link.${link}`)}
    </span>
  );
}
