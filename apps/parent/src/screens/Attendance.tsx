import { useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { monthLabel } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { hubApi } from "@parent/api";
import type { Counts, DayStatus } from "@parent/api";
import { monthOf, percent, shiftMonth, thisMonth } from "@parent/format";
import { LastUpdated, Loadable, Panel } from "@parent/ui";
import { useChild } from "./Child";

const TONE: Record<DayStatus, BadgeTone> = {
  present: "success",
  absent: "danger",
  late: "warning",
  leave: "lavender",
};
const STATUSES: DayStatus[] = ["present", "absent", "late", "leave"];

function Summary({ counts, testId = "count" }: { counts: Counts; testId?: string }) {
  const { t } = useI18n();
  return (
    <dl className="grid grid-cols-4 gap-2 text-center">
      {STATUSES.map((s) => (
        <div key={s} className="rounded border border-border px-1 py-2">
          <dt className="text-2xs text-muted">{t(`parent.attendance.${s}` as TranslationKey)}</dt>
          <dd className="text-base font-semibold text-text" data-testid={`${testId}-${s}`}>
            {counts[s]}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** Attendance, one month at a time, with the year so far below. */
export function AttendanceScreen() {
  const { t, locale, formatDate } = useI18n();
  const { child } = useChild();
  const [month, setMonth] = useState<string | undefined>(undefined);
  const data = useAsync(() => hubApi.attendance(child, month), [child, month]);
  const intl = locale === "ur" ? "ur-PK" : "en-PK";
  const shown = data.data?.month ?? month ?? thisMonth();
  const year = data.data?.year ?? null;
  // No further back than the school year, no further on than this month.
  const canGoBack = !year || shown > monthOf(year.start_date);
  const canGoOn = shown < thisMonth();
  const step = (by: number) => setMonth(shiftMonth(shown, by));
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between rounded-lg border border-border bg-card px-1 py-1">
        <button
          type="button"
          onClick={() => step(-1)}
          disabled={!canGoBack}
          aria-label={t("parent.attendance.previous")}
          className="flex h-11 w-11 items-center justify-center rounded text-muted hover:bg-hover disabled:opacity-40"
          data-testid="month-previous"
        >
          <ChevronLeft className="h-5 w-5 rtl:rotate-180" aria-hidden />
        </button>
        <h2 className="text-sm font-semibold text-text" aria-live="polite" data-testid="month">
          {monthLabel(`${shown}-01`, intl)}
        </h2>
        <button
          type="button"
          onClick={() => step(1)}
          disabled={!canGoOn}
          aria-label={t("parent.attendance.next")}
          className="flex h-11 w-11 items-center justify-center rounded text-muted hover:bg-hover disabled:opacity-40"
          data-testid="month-next"
        >
          <ChevronRight className="h-5 w-5 rtl:rotate-180" aria-hidden />
        </button>
      </div>
      <Loadable state={data}>
        {(a) => (
          <>
            <Panel
              title={t("parent.attendance.thisMonth")}
              aside={
                a.summary.total > 0 && (
                  <span className="text-sm font-semibold text-text">
                    {percent(a.summary.percent)}
                  </span>
                )
              }
            >
              <Summary counts={a.summary} />
            </Panel>
            <Panel title={t("parent.attendance.days")}>
              {a.days.length === 0 ? (
                <p className="py-4 text-center text-sm text-muted">{t("parent.attendance.none")}</p>
              ) : (
                <ul className="divide-y divide-border">
                  {a.days.map((d) => (
                    <li key={d.date} className="flex items-center justify-between gap-3 py-2">
                      <span className="text-sm text-text">
                        {formatDate(`${d.date}T00:00:00`, {
                          weekday: "short",
                          day: "numeric",
                          month: "short",
                        })}
                      </span>
                      <Badge tone={TONE[d.status]}>
                        {t(`parent.attendance.${d.status}` as TranslationKey)}
                      </Badge>
                    </li>
                  ))}
                </ul>
              )}
            </Panel>
            {a.year && a.year.summary.total > 0 && (
              <Panel
                title={t("parent.attendance.year", { year: a.year.name })}
                aside={
                  <span className="text-sm font-semibold text-text" data-testid="year-percent">
                    {percent(a.year.summary.percent)}
                  </span>
                }
              >
                <Summary counts={a.year.summary} testId="year-count" />
              </Panel>
            )}
            <LastUpdated at={a.synced_at} />
          </>
        )}
      </Loadable>
    </div>
  );
}
