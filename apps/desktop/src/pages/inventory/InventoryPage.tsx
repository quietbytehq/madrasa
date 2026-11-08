import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { AssetsTab } from "./AssetsTab";
import { ItemsTab } from "./ItemsTab";
import { MovementsTab } from "./MovementsTab";
import { SetupTab } from "./SetupTab";
import { StockTab } from "./StockTab";

const TABS: { key: string; labelKey: TranslationKey; manage?: boolean }[] = [
  { key: "items", labelKey: "inv.tab.items" },
  { key: "movements", labelKey: "inv.tab.movements" },
  { key: "stock", labelKey: "inv.tab.stock" },
  { key: "assets", labelKey: "inv.tab.assets" },
  { key: "setup", labelKey: "inv.tab.setup", manage: true },
];

/** The Inventory (store) module (Standard stage 7). */
export function InventoryPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("inventory.manage");
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "items");
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  return (
    <>
      <PageHeader
        title={t("inv.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("inv.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("inv.title")}
        className="mb-3 flex gap-1 border-b border-border print:hidden"
      >
        {TABS.filter((tb) => manage || !tb.manage).map((tb) => (
          <button
            key={tb.key}
            role="tab"
            type="button"
            aria-selected={tab === tb.key}
            onClick={() => select(tb.key)}
            className={`-mb-px border-b-2 px-3 py-2 text-sm ${
              tab === tb.key
                ? "border-blue font-medium text-blue"
                : "border-transparent text-muted hover:text-text"
            }`}
          >
            {t(tb.labelKey)}
          </button>
        ))}
      </div>
      {tab === "items" && <ItemsTab manage={manage} />}
      {tab === "movements" && <MovementsTab manage={manage} />}
      {tab === "stock" && <StockTab />}
      {tab === "assets" && <AssetsTab manage={manage} />}
      {tab === "setup" && manage && <SetupTab />}
    </>
  );
}
