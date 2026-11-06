import { api } from "@/api/endpoints";
import { Card } from "@/components/ui/Card";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { num } from "./common";

/** A student's board results, one card per exam, under the school's own report card. */
export function BoardResultsView({ studentId }: { studentId: string }) {
  const { t } = useI18n();
  const results = useAsync(() => api.studentBoardResults(studentId), [studentId]);
  if (!results.data || results.data.length === 0) return null;
  return (
    <Card title={t("boards.student.title")}>
      <div className="flex flex-col gap-4">
        {results.data.map((r) => (
          <div key={r.exam.id}>
            <div className="mb-1 text-sm font-semibold text-text">
              {r.board_name} · {r.exam.name}
              {r.row.candidate.roll_no
                ? ` · ${t("boards.candidates.rollNo")} ${r.row.candidate.roll_no}`
                : ""}
            </div>
            <table className="w-full text-sm">
              <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-2 py-1 text-start">{t("boards.exam.subjectName")}</th>
                  <th className="px-2 py-1 text-end">{t("boards.exam.maxMarks")}</th>
                  <th className="px-2 py-1 text-end">{t("boards.results.field.obtained")}</th>
                  <th className="px-2 py-1 text-start">{t("boards.results.grade")}</th>
                  <th className="px-2 py-1 text-start">{t("boards.results.result")}</th>
                </tr>
              </thead>
              <tbody>
                {r.exam.subjects.map((s, i) => {
                  const cell = r.row.cells[i];
                  return (
                    <tr key={s.id} className="border-t border-border">
                      <td className="px-2 py-1">{s.name}</td>
                      <td className="px-2 py-1 text-end">{num(s.max_marks)}</td>
                      <td className="px-2 py-1 text-end">
                        {cell?.obtained ? num(cell.obtained) : "—"}
                      </td>
                      <td className="px-2 py-1">{cell?.grade ?? "—"}</td>
                      <td className="px-2 py-1">
                        {cell?.status ? t(`boards.status.${cell.status}`) : "—"}
                      </td>
                    </tr>
                  );
                })}
                <tr className="border-t border-border bg-hover font-semibold">
                  <td className="px-2 py-1">{t("boards.results.total")}</td>
                  <td className="px-2 py-1 text-end">{num(r.row.max)}</td>
                  <td className="px-2 py-1 text-end">{num(r.row.total)}</td>
                  <td className="px-2 py-1">{r.row.grade ?? "—"}</td>
                  <td className="px-2 py-1">
                    {t(`boards.status.${r.row.status}`)}
                    {r.row.percent ? ` · ${num(r.row.percent)}%` : ""}
                    {r.row.position ? ` · #${r.row.position}` : ""}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        ))}
      </div>
    </Card>
  );
}
