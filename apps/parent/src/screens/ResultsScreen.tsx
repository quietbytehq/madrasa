import type { ReportCard } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { hubApi } from "@parent/api";
import { marks, percent } from "@parent/format";
import { LastUpdated, Loadable, Panel } from "@parent/ui";
import { useChild } from "./Child";

/** One assessment of the report card: every subject's marks and the total. */
function Assessment({ card, index }: { card: ReportCard; index: number }) {
  const { t } = useI18n();
  const assessment = card.assessments[index]!;
  const total = card.totals[index];
  const rows = card.subjects
    .map((s) => ({ subject: s, cell: s.cells[index] }))
    .filter((r) => r.cell && r.cell.max_marks !== null);
  return (
    <Panel
      title={assessment.name}
      aside={
        total?.grade && (
          <Badge tone="info">{t("parent.results.grade", { grade: total.grade })}</Badge>
        )
      }
    >
      <table className="w-full text-sm" data-testid={`assessment-${index}`}>
        <thead>
          <tr className="text-xs text-muted">
            <th scope="col" className="py-1 text-start font-medium">
              {t("parent.results.subject")}
            </th>
            <th scope="col" className="py-1 text-end font-medium">
              {t("parent.results.marks")}
            </th>
            <th scope="col" className="py-1 text-end font-medium">
              %
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {rows.map(({ subject, cell }) => (
            <tr key={subject.subject_id}>
              <th scope="row" className="py-1.5 text-start font-normal text-text">
                {subject.subject_name}
              </th>
              <td className="py-1.5 text-end text-text" dir="ltr">
                {cell!.is_absent
                  ? t("parent.results.absent")
                  : `${marks(cell!.obtained)} / ${marks(cell!.max_marks)}`}
              </td>
              <td className="py-1.5 text-end text-text" dir="ltr">
                {cell!.is_absent ? "—" : percent(cell!.percent)}
              </td>
            </tr>
          ))}
        </tbody>
        {total && (
          <tfoot>
            <tr className="border-t border-border font-semibold text-text">
              <th scope="row" className="py-1.5 text-start">
                {t("parent.results.total")}
              </th>
              <td className="py-1.5 text-end" dir="ltr">
                {marks(total.total)} / {marks(total.max)}
              </td>
              <td className="py-1.5 text-end" dir="ltr">
                {percent(total.percent)}
              </td>
            </tr>
          </tfoot>
        )}
      </table>
      {total?.position != null && total.students > 0 && (
        <p className="mt-2 text-xs text-muted" data-testid={`position-${index}`}>
          {t("parent.results.position", { position: total.position, students: total.students })}
        </p>
      )}
    </Panel>
  );
}

/**
 * Results and the report card for the school year: the school's own figures, from marks
 * the school has approved. Marks still being entered or checked do not appear at all.
 */
export function ResultsScreen() {
  const { t } = useI18n();
  const { child } = useChild();
  const data = useAsync(() => hubApi.results(child), [child]);
  return (
    <Loadable state={data}>
      {({ card, synced_at }) => {
        const held = card
          ? card.assessments.map((a, i) => (a.held ? i : -1)).filter((i) => i >= 0)
          : [];
        return (
          <div className="flex flex-col gap-2">
            {!card ? (
              <p className="px-4 py-10 text-center text-sm text-muted">
                {t("parent.results.noYear")}
              </p>
            ) : held.length === 0 ? (
              <div className="px-4 py-10 text-center" data-testid="no-results">
                <p className="text-sm font-medium text-text">{t("parent.results.none")}</p>
                <p className="mt-2 text-sm text-muted">{t("parent.results.none.body")}</p>
              </div>
            ) : (
              <>
                <p className="px-1 text-xs text-muted">
                  {t("parent.results.year", { year: card.academic_year })}
                </p>
                {held.map((i) => (
                  <Assessment key={card.assessments[i]!.id} card={card} index={i} />
                ))}
                {card.overall_percent !== null && (
                  <Panel
                    title={t("parent.results.overall")}
                    aside={
                      card.pass !== null && (
                        <Badge tone={card.pass ? "success" : "danger"}>
                          {card.pass ? t("parent.results.pass") : t("parent.results.fail")}
                        </Badge>
                      )
                    }
                  >
                    <div className="flex items-baseline gap-3" data-testid="overall">
                      <span className="text-2xl font-semibold text-text" dir="ltr">
                        {percent(card.overall_percent)}
                      </span>
                      {card.overall_grade && (
                        <span className="text-sm text-text">
                          {t("parent.results.grade", { grade: card.overall_grade })}
                        </span>
                      )}
                      {card.overall_remarks && (
                        <span className="text-sm text-muted">{card.overall_remarks}</span>
                      )}
                    </div>
                    <p className="mt-2 text-xs text-muted">{t("parent.results.overall.note")}</p>
                  </Panel>
                )}
                <p className="px-1 text-xs text-muted">{t("parent.results.approvedOnly")}</p>
              </>
            )}
            <LastUpdated at={synced_at} />
          </div>
        );
      }}
    </Loadable>
  );
}
