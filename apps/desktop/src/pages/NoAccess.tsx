import { Lock } from "lucide-react";
import { useI18n } from "@/i18n";
import { EmptyState } from "@/components/ui/EmptyState";

/** Shown in place of a page the signed-in role may not use. */
export function NoAccess() {
  const { t } = useI18n();
  return (
    <div className="py-10">
      <EmptyState
        icon={<Lock className="h-8 w-8" aria-hidden />}
        title={t("app.noAccess.title")}
        description={t("app.noAccess.body")}
      />
    </div>
  );
}
