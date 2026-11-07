import { useSearchParams } from "react-router-dom";
import { api } from "@/api/endpoints";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PrintToolbar } from "../fees/PrintPages";
import { ReportCardView } from "./ReportCardView";
import { ResultSheetView } from "./tabs/ResultsTab";

/** `/exams/report-cards/print?section_id=…` (every student) or `&student_id=…` (one). */
export function ReportCardsPrintPage() {
  const { t, locale } = useI18n();
  const [params] = useSearchParams();
  const sectionId = params.get("section_id") ?? "";
  const studentId = params.get("student_id");
  const cards = useAsync(
    () =>
      studentId
        ? api.studentReportCard(studentId).then((c) => [c])
        : api.sectionReportCards(sectionId),
    [sectionId, studentId],
  );
  if (cards.loading && !cards.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (cards.error || !cards.data)
    return <EmptyState title={errorMessage(cards.error, t("common.error.load"))} />;
  return (
    <>
      <PrintToolbar
        title={t("exams.card.title")}
        onPdf={() =>
          api.reportCardsPdf(
            studentId ? { student_id: studentId } : { section_id: sectionId },
            locale === "ur" ? "ur" : "en",
          )
        }
      />
      <p className="mb-2 text-xs text-muted print:hidden">{t("fees.print.downloadHint")}</p>
      <div className="print-area flex flex-col gap-4">
        {cards.data.map((c) => (
          <ReportCardView key={c.student_id} card={c} t={t} print />
        ))}
      </div>
    </>
  );
}

/** `/exams/results/print?assessment_id=…&section_id=…` */
export function ResultSheetPrintPage() {
  const { t } = useI18n();
  const [params] = useSearchParams();
  const assessmentId = params.get("assessment_id") ?? "";
  const sectionId = params.get("section_id") ?? "";
  const r = useAsync(() => api.sectionResults(assessmentId, sectionId), [assessmentId, sectionId]);
  if (r.loading && !r.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (r.error || !r.data)
    return <EmptyState title={errorMessage(r.error, t("common.error.load"))} />;
  return (
    <>
      <PrintToolbar
        title={`${r.data.assessment.name} · ${r.data.section.class_name}-${r.data.section.section_name}`}
      />
      <div className="print-area">
        <ResultSheetView r={r.data} t={t} print />
      </div>
    </>
  );
}
