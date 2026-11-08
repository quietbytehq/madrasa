import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { BoardTab } from "./BoardTab";
import { HostelsTab } from "./HostelsTab";
import { RegisterTab } from "./RegisterTab";
import { ResidentsTab } from "./ResidentsTab";

const TABS: { key: string; labelKey: TranslationKey }[] = [
  { key: "board", labelKey: "hst.tab.board" },
  { key: "residents", labelKey: "hst.tab.residents" },
  { key: "register", labelKey: "hst.tab.register" },
  { key: "hostels", labelKey: "hst.tab.hostels" },
];

/** The Hostel module (Standard stage 6). */
export function HostelPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("hostel.manage");
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "board");
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  return (
    <>
      <PageHeader
        title={t("hst.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("hst.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("hst.title")}
        className="mb-3 flex gap-1 border-b border-border print:hidden"
      >
        {TABS.map((tb) => (
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
      {tab === "board" && <BoardTab manage={manage} />}
      {tab === "residents" && <ResidentsTab manage={manage} />}
      {tab === "register" && <RegisterTab manage={manage} />}
      {tab === "hostels" && <HostelsTab manage={manage} />}
    </>
  );
}
