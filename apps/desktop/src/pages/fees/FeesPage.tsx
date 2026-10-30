import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Card } from "@/components/ui/Card";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { CollectTab } from "./tabs/CollectTab";
import { CollectionsTab } from "./tabs/CollectionsTab";
import { DefaultersTab } from "./tabs/DefaultersTab";
import { SetupTab } from "./tabs/SetupTab";
import { VouchersTab } from "./tabs/VouchersTab";

const TABS: Array<{ key: string; labelKey: TranslationKey; permission: string }> = [
  { key: "vouchers", labelKey: "fees.tabs.vouchers", permission: "fees.read" },
  { key: "collect", labelKey: "fees.tabs.collect", permission: "fees.read" },
  { key: "defaulters", labelKey: "fees.tabs.defaulters", permission: "fees.read" },
  { key: "collections", labelKey: "fees.tabs.collections", permission: "fees.read" },
  { key: "setup", labelKey: "fees.tabs.setup", permission: "fees.read" },
];

export function FeesPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "vouchers");
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  return (
    <>
      <PageHeader
        title={t("fees.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("fees.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("fees.title")}
        className="mb-3 flex gap-1 border-b border-border"
      >
        {TABS.filter((tb) => can(tb.permission)).map((tb) => (
          <button
            key={tb.key}
            role="tab"
            type="button"
            aria-selected={tab === tb.key}
            onClick={() => select(tb.key)}
            className={cn(
              "-mb-px border-b-2 px-3 py-2 text-sm",
              tab === tb.key
                ? "border-blue font-medium text-blue"
                : "border-transparent text-muted hover:text-text",
            )}
          >
            {t(tb.labelKey)}
          </button>
        ))}
      </div>
      <div role="tabpanel">
        {tab === "vouchers" && <VouchersTab />}
        {tab === "collect" && <CollectTab />}
        {tab === "defaulters" && <DefaultersTab />}
        {tab === "collections" && <CollectionsTab />}
        {tab === "setup" && (can("fees.read") ? <SetupTab /> : <Card />)}
      </div>
    </>
  );
}
