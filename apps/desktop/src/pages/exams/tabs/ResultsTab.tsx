import { useCallback, useState } from "react";
import { Link } from "react-router-dom";
import { Printer } from "lucide-react";
import { api } from "@/api/endpoints";
import type { ResultSheet } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import type { TFunction } from "@/i18n";
import { cn } from "@/lib/cn";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { AssessmentSectionPickers, num, pct, SheetStatusBadge, useExamContext } from "../common";

export function ResultsTab() {
  const { t } = useI18n();
  const ctx = useExamContext();
  const [sel, setSel] = useState({ assessmentId: "", sectionId: "" });
  const onChange = useCallback((n: { assessmentId: string; sectionId: string }) => setSel(n), []);
  const result = useAsync(
    () =>
      sel.assessmentId && sel.sectionId
        ? api.sectionResults(sel.assessmentId, sel.sectionId)
        : Promise.resolve(null),
    [sel.assessmentId, sel.sectionId],
  );
  if (!ctx.year && !ctx.loading) return <EmptyState title={t("exams.setup.noYear")} />;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <AssessmentSectionPickers
          assessments={ctx.assessments}
          sections={ctx.sections}
          assessmentId={sel.assessmentId}
          sectionId={sel.sectionId}
          onChange={onChange}
        />
        <span className="flex-1" />
        {result.data && (
          <Link
            to={`/exams/results/print?assessment_id=${sel.assessmentId}&section_id=${sel.sectionId}`}
          >
            <Button variant="secondary" icon={<Printer className="h-4 w-4" aria-hidden />}>
              {t("exams.results.print")}
            </Button>
          </Link>
        )}
      </div>
      {result.loading && !result.data ? (
        <div className="flex justify-center py-10">
          <Spinner />
        </div>
      ) : result.error ? (
        <EmptyState title={errorMessage(result.error, t("common.error.load"))} />
      ) : result.data ? (
        <ResultSheetView r={result.data} t={t} />
      ) : null}
    </div>
  );
}

/** Result sheet + subject analysis; also used by the print page. */
export function ResultSheetView({
  r,
  t,
  print,
}: {
  r: ResultSheet;
  t: TFunction;
  print?: boolean;
}) {
  return (
    <div className="flex flex-col gap-3">
      {!print && (
        <p className={cn("text-xs", r.complete ? "text-success" : "text-warning")} role="status">
          {r.complete ? t("exams.results.complete") : t("exams.results.incomplete")}
        </p>
      )}
      <Card padded={false} className={cn(print && "rounded-none border-black")}>
        {r.rows.length === 0 ? (
          <EmptyState title={t("exams.results.empty")} />
        ) : (
          <table className="w-full text-sm">
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-2 py-2 text-start">{t("attendance.mark.rollNo")}</th>
                <th className="px-2 py-2 text-start">{t("students.fullName")}</th>
                {r.subjects.map((s) => (
                  <th key={s.subject_id} className="px-2 py-2 text-end">
                    <div>{s.subject_name}</div>
                    <div className="font-normal normal-case">
                      /{num(s.max_marks)}{" "}
                      {!print && s.sheet_status !== "approved" && (
                        <SheetStatusBadge status={s.sheet_status} t={t} />
                      )}
                    </div>
                  </th>
                ))}
                <th className="px-2 py-2 text-end">{t("exams.results.total")}</th>
                <th className="px-2 py-2 text-end">{t("exams.results.percent")}</th>
                <th className="px-2 py-2 text-start">{t("exams.results.grade")}</th>
                <th className="px-2 py-2 text-end">{t("exams.results.position")}</th>
              </tr>
            </thead>
            <tbody>
              {r.rows.map((row) => (
                <tr key={row.student_id} className="border-t border-border">
                  <td className="px-2 py-1 tabular-nums text-muted">{row.roll_no ?? "—"}</td>
                  <td className="px-2 py-1 font-medium">{row.full_name}</td>
                  {row.marks.map((m) => (
                    <td
                      key={m.subject_id}
                      className={cn(
                        "px-2 py-1 text-end tabular-nums",
                        m.is_absent && "text-danger",
                      )}
                    >
                      {m.is_absent ? t("attendance.short.absent") : num(m.obtained)}
                    </td>
                  ))}
                  <td className="px-2 py-1 text-end tabular-nums">
                    {num(row.total)}/{num(row.max)}
                  </td>
                  <td className="px-2 py-1 text-end tabular-nums">{pct(row.percent)}</td>
                  <td className="px-2 py-1">
                    <Badge tone={row.pass ? "success" : "danger"}>{row.grade}</Badge>
                  </td>
                  <td className="px-2 py-1 text-end tabular-nums">{row.position}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
      <Card
        title={t("exams.results.analysis")}
        padded={false}
        className={cn(print && "rounded-none border-black")}
      >
        <table className="w-full text-sm">
          <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-2 py-2 text-start">{t("exams.subject")}</th>
              <th className="px-2 py-2 text-end">{t("exams.results.appeared")}</th>
              <th className="px-2 py-2 text-end">{t("exams.results.absent")}</th>
              <th className="px-2 py-2 text-end">{t("exams.results.average")}</th>
              <th className="px-2 py-2 text-end">{t("exams.results.highest")}</th>
              <th className="px-2 py-2 text-end">{t("exams.results.lowest")}</th>
              <th className="px-2 py-2 text-end">{t("exams.results.passed")}</th>
              <th className="px-2 py-2 text-end">{t("exams.results.failed")}</th>
            </tr>
          </thead>
          <tbody>
            {r.analysis.map((a) => (
              <tr key={a.subject_id} className="border-t border-border">
                <td className="px-2 py-1 font-medium">{a.subject_name}</td>
                <td className="px-2 py-1 text-end tabular-nums">{a.appeared}</td>
                <td className="px-2 py-1 text-end tabular-nums">{a.absent}</td>
                <td className="px-2 py-1 text-end tabular-nums">{pct(a.average_percent)}</td>
                <td className="px-2 py-1 text-end tabular-nums">{num(a.highest)}</td>
                <td className="px-2 py-1 text-end tabular-nums">{num(a.lowest)}</td>
                <td className="px-2 py-1 text-end tabular-nums">{a.passed}</td>
                <td className="px-2 py-1 text-end tabular-nums">{a.failed}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
    </div>
  );
}
