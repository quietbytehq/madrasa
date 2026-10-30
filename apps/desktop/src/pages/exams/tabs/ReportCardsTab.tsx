import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Printer } from "lucide-react";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { pct, useExamContext } from "../common";

export function ReportCardsTab() {
  const { t } = useI18n();
  const ctx = useExamContext();
  const [sectionId, setSectionId] = useState("");
  useEffect(() => {
    if (!sectionId && ctx.sections.length > 0) setSectionId(ctx.sections[0]!.section_id);
  }, [ctx.sections, sectionId]);
  const cards = useAsync(
    () => (sectionId ? api.sectionReportCards(sectionId) : Promise.resolve(null)),
    [sectionId],
  );
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <Select
          label={t("exams.section")}
          value={sectionId}
          onChange={(e) => setSectionId(e.target.value)}
          options={ctx.sections.map((s) => ({
            value: s.section_id,
            label: `${s.class_name}-${s.section_name}`,
          }))}
          className="w-44"
        />
        <span className="flex-1" />
        {sectionId && (
          <Link to={`/exams/report-cards/print?section_id=${sectionId}`}>
            <Button icon={<Printer className="h-4 w-4" aria-hidden />}>
              {t("exams.card.printAll")}
            </Button>
          </Link>
        )}
      </div>
      <Card padded={false}>
        {cards.loading && !cards.data ? (
          <div className="flex justify-center py-10">
            <Spinner />
          </div>
        ) : cards.error ? (
          <EmptyState title={errorMessage(cards.error, t("common.error.load"))} />
        ) : !cards.data || cards.data.length === 0 ? (
          <EmptyState title={t("exams.results.empty")} />
        ) : (
          <table className="w-full text-sm">
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2 text-start">{t("attendance.mark.rollNo")}</th>
                <th className="px-3 py-2 text-start">{t("students.fullName")}</th>
                <th className="px-3 py-2 text-end">{t("exams.card.overall")}</th>
                <th className="px-3 py-2 text-start">{t("exams.results.grade")}</th>
                <th className="px-3 py-2 text-end">{t("common.actions")}</th>
              </tr>
            </thead>
            <tbody>
              {cards.data.map((c) => (
                <tr key={c.student_id} className="border-t border-border">
                  <td className="px-3 py-1.5 tabular-nums text-muted">{c.roll_no ?? "—"}</td>
                  <td className="px-3 py-1.5">
                    <Link to={`/students/${c.student_id}`} className="font-medium hover:underline">
                      {c.full_name}
                    </Link>
                  </td>
                  <td className="px-3 py-1.5 text-end tabular-nums">{pct(c.overall_percent)}</td>
                  <td className="px-3 py-1.5">{c.overall_grade ?? "—"}</td>
                  <td className="px-3 py-1.5 text-end">
                    <Link
                      to={`/exams/report-cards/print?section_id=${sectionId}&student_id=${c.student_id}`}
                      className="text-blue hover:underline"
                    >
                      {t("exams.card.print")}
                    </Link>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}
