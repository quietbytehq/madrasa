import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { AssignTab } from "./AssignTab";
import { RosterTab } from "./RosterTab";
import { RoutesTab } from "./RoutesTab";
import { SettingsTab } from "./SettingsTab";
import { VehiclesTab } from "./VehiclesTab";

const TABS: { key: string; labelKey: TranslationKey; manage?: boolean }[] = [
  { key: "routes", labelKey: "trp.tab.routes" },
  { key: "assign", labelKey: "trp.tab.assign" },
  { key: "roster", labelKey: "trp.tab.roster" },
  { key: "vehicles", labelKey: "trp.tab.vehicles" },
  { key: "settings", labelKey: "trp.tab.settings", manage: true },
];

/** The Transport module (Standard stage 5). */
export function TransportPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("transport.manage");
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "routes");
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  return (
    <>
      <PageHeader
        title={t("trp.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("trp.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("trp.title")}
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
      {tab === "routes" && <RoutesTab manage={manage} />}
      {tab === "assign" && <AssignTab manage={manage} />}
      {tab === "roster" && <RosterTab />}
      {tab === "vehicles" && <VehiclesTab manage={manage} />}
      {tab === "settings" && manage && <SettingsTab />}
    </>
  );
}
