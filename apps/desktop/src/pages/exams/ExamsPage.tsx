import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { MarksTab } from "./tabs/MarksTab";
import { ReportCardsTab } from "./tabs/ReportCardsTab";
import { ResultsTab } from "./tabs/ResultsTab";
import { SetupTab } from "./tabs/SetupTab";

const TABS: Array<{ key: string; labelKey: TranslationKey; permission: string }> = [
  { key: "marks", labelKey: "exams.tabs.marks", permission: "exams.read" },
  { key: "results", labelKey: "exams.tabs.results", permission: "exams.read" },
  { key: "cards", labelKey: "exams.tabs.reportCards", permission: "exams.read" },
  { key: "setup", labelKey: "exams.tabs.setup", permission: "exams.read" },
];

export function ExamsPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "marks");
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  return (
    <>
      <PageHeader
        title={t("exams.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("exams.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("exams.title")}
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
        {tab === "marks" && <MarksTab />}
        {tab === "results" && <ResultsTab />}
        {tab === "cards" && <ReportCardsTab />}
        {tab === "setup" && <SetupTab />}
      </div>
    </>
  );
}
