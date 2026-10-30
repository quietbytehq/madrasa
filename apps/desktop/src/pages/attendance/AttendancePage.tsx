import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { AbsenteesTab } from "./tabs/AbsenteesTab";
import { CalendarTab } from "./tabs/CalendarTab";
import { MarkTab } from "./tabs/MarkTab";
import { RegisterTab } from "./tabs/RegisterTab";
import { StaffTab } from "./tabs/StaffTab";

const TABS: Array<{ key: string; labelKey: TranslationKey; permission: string }> = [
  { key: "mark", labelKey: "attendance.tabs.mark", permission: "attendance.read" },
  { key: "register", labelKey: "attendance.tabs.register", permission: "attendance.read" },
  { key: "absentees", labelKey: "attendance.tabs.absentees", permission: "attendance.read" },
  { key: "staff", labelKey: "attendance.tabs.staff", permission: "teachers.read" },
  { key: "calendar", labelKey: "attendance.tabs.calendar", permission: "attendance.read" },
];

export function AttendancePage() {
  const { t } = useI18n();
  const { can } = useSession();
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "mark");
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  return (
    <>
      <PageHeader
        title={t("attendance.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("attendance.title") }]}
      />
      <div
        role="tablist"
        aria-label={t("attendance.title")}
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
        {tab === "mark" && <MarkTab initialSection={params.get("section") ?? undefined} />}
        {tab === "register" && <RegisterTab />}
        {tab === "absentees" && <AbsenteesTab />}
        {tab === "staff" && can("teachers.read") && <StaffTab />}
        {tab === "calendar" && <CalendarTab />}
      </div>
    </>
  );
}
