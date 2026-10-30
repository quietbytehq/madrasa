import { Construction } from "lucide-react";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Badge } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { PageHeader } from "./PageHeader";

export function ComingSoon({ labelKey }: { labelKey: TranslationKey }) {
  const { t } = useI18n();
  const title = t(labelKey);
  return (
    <>
      <PageHeader
        title={title}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: title }]}
        actions={<Badge tone="warning">{t("app.comingSoon")}</Badge>}
      />
      <Card>
        <EmptyState
          icon={<Construction className="h-8 w-8" aria-hidden />}
          title={t("app.comingSoon")}
          description={t("app.comingSoonBody")}
        />
      </Card>
    </>
  );
}
