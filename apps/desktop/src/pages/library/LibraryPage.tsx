import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { CatalogueTab } from "./CatalogueTab";
import { DeskTab } from "./DeskTab";
import { LoansTab } from "./LoansTab";
import { SettingsTab } from "./SettingsTab";
import { StockTab } from "./StockTab";

const TABS: { key: string; labelKey: TranslationKey; manage?: boolean }[] = [
  { key: "desk", labelKey: "lib.tab.desk" },
  { key: "catalogue", labelKey: "lib.tab.catalogue" },
  { key: "loans", labelKey: "lib.tab.loans" },
  { key: "stock", labelKey: "lib.tab.stock" },
  { key: "settings", labelKey: "lib.tab.settings", manage: true },
];

/** The Library module (Standard stage 4). */
export function LibraryPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("library.manage");
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "desk");
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  return (
    <>
      <PageHeader
        title={t("lib.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("lib.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("lib.title")}
        className="mb-3 flex gap-1 border-b border-border"
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
      {tab === "catalogue" ? (
        <CatalogueTab manage={manage} />
      ) : tab === "loans" ? (
        <LoansTab manage={manage} />
      ) : tab === "stock" ? (
        <StockTab manage={manage} />
      ) : tab === "settings" && manage ? (
        <SettingsTab />
      ) : (
        <DeskTab manage={manage} />
      )}
    </>
  );
}
