import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { BalancesTab } from "./BalancesTab";
import { RequestsTab } from "./RequestsTab";
import { StaffRegisterTab } from "./StaffRegisterTab";
import { StudentLeaveTab } from "./StudentLeaveTab";
import { TypesTab } from "./TypesTab";

const TABS: { key: string; labelKey: TranslationKey; manage?: boolean }[] = [
  { key: "requests", labelKey: "leave.tab.requests" },
  { key: "students", labelKey: "leave.tab.students" },
  { key: "balances", labelKey: "leave.tab.balances" },
  { key: "register", labelKey: "leave.tab.register" },
  { key: "types", labelKey: "leave.tab.types", manage: true },
];

/** The Leave module (Standard stage 2). */
export function LeavePage() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("leave.manage");
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "requests");
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  return (
    <>
      <PageHeader
        title={t("leave.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("leave.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("leave.title")}
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
      {tab === "students" ? (
        <StudentLeaveTab showStudent />
      ) : tab === "balances" ? (
        <BalancesTab manage={manage} />
      ) : tab === "register" ? (
        <StaffRegisterTab />
      ) : tab === "types" && manage ? (
        <TypesTab />
      ) : (
        <RequestsTab manage={manage} />
      )}
    </>
  );
}
