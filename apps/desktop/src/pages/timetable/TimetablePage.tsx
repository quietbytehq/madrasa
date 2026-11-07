import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "@/api/endpoints";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSession } from "@/app/session";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";
import { GenerateTab } from "./GenerateTab";
import { LessonsTab } from "./LessonsTab";
import { SetupTab } from "./SetupTab";
import { SubstitutionsTab } from "./SubstitutionsTab";
import { ViewTab } from "./ViewTab";

const TABS: { key: string; labelKey: TranslationKey; manage?: boolean }[] = [
  { key: "view", labelKey: "tt.tab.view" },
  { key: "substitutions", labelKey: "tt.tab.substitutions" },
  { key: "setup", labelKey: "tt.tab.setup", manage: true },
  { key: "lessons", labelKey: "tt.tab.lessons", manage: true },
  { key: "generate", labelKey: "tt.tab.generate", manage: true },
];

/** The Timetable module (Standard stage 1). */
export function TimetablePage() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("timetable.manage");
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "view");
  const [timetableId, setTimetableId] = useState(params.get("timetable") ?? "");
  const setup = useAsync(() => api.timetableSetup(), []);
  const select = (key: string, timetable?: string) => {
    setTab(key);
    if (timetable !== undefined) setTimetableId(timetable);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    if (timetable) next.set("timetable", timetable);
    else if (timetable === "") next.delete("timetable");
    setParams(next, { replace: true });
  };
  const tabs = TABS.filter((tb) => manage || !tb.manage);
  return (
    <>
      <PageHeader
        title={t("tt.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("tt.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("tt.title")}
        className="mb-3 flex gap-1 border-b border-border"
      >
        {tabs.map((tb) => (
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
      {setup.loading && !setup.data ? (
        <div className="flex justify-center py-10">
          <Spinner size="lg" />
        </div>
      ) : setup.error || !setup.data ? (
        <Card>
          <EmptyState title={errorMessage(setup.error, t("tt.noYear"))} />
        </Card>
      ) : tab === "setup" ? (
        <SetupTab setup={setup.data} manage={manage} reload={setup.reload} />
      ) : tab === "lessons" ? (
        <LessonsTab setup={setup.data} manage={manage} reload={setup.reload} />
      ) : tab === "generate" ? (
        <GenerateTab
          setup={setup.data}
          manage={manage}
          reload={setup.reload}
          onOpen={(id) => select("view", id)}
        />
      ) : tab === "substitutions" ? (
        <SubstitutionsTab setup={setup.data} manage={manage} />
      ) : (
        <ViewTab
          setup={setup.data}
          manage={manage}
          timetableId={timetableId}
          onPickTimetable={(id) => select("view", id)}
        />
      )}
    </>
  );
}
