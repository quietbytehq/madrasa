import { Link } from "react-router-dom";
import { Printer } from "lucide-react";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { ReportCardView } from "./ReportCardView";

/** Results tab on the student profile: the report card for the active year. */
export function StudentResultsView({ studentId }: { studentId: string }) {
  const { t } = useI18n();
  const card = useAsync(() => api.studentReportCard(studentId), [studentId]);
  if (card.loading && !card.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (card.error || !card.data)
    return <EmptyState title={errorMessage(card.error, t("common.error.load"))} />;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex justify-end">
        <Link to={`/exams/report-cards/print?student_id=${studentId}`}>
          <Button variant="secondary" icon={<Printer className="h-4 w-4" aria-hidden />}>
            {t("exams.card.print")}
          </Button>
        </Link>
      </div>
      <ReportCardView card={card.data} t={t} />
    </div>
  );
}
