import type { ReportCard } from "@/api/types";
import type { TFunction } from "@/i18n";
import { cn } from "@/lib/cn";
import { num, pct } from "./common";

/** Default report card design, used on screen and in print (one card per page). */
export function ReportCardView({
  card,
  t,
  print,
}: {
  card: ReportCard;
  t: TFunction;
  print?: boolean;
}) {
  const held = card.assessments.filter((a) => a.held);
  const weights = card.assessments.map((a) => `${a.name} ${num(a.weight)}%`).join(", ");
  return (
    <section
      className={cn(
        "report-card rounded border border-border bg-card p-5 text-sm text-text",
        print && "rounded-none border-black print:break-after-page",
      )}
      data-testid="report-card"
    >
      <header className="mb-3 border-b border-border pb-2 text-center">
        <div className="text-lg font-bold text-navy">{card.school.name}</div>
        <div className="text-xs text-muted">
          {card.school.city} · {card.school.code}
        </div>
        <div className="mt-1 font-semibold uppercase tracking-wide">
          {t("exams.card.title")} · {card.academic_year}
        </div>
      </header>
      <dl className="mb-3 grid grid-cols-2 gap-x-6 gap-y-1 text-xs md:grid-cols-4">
        <dt className="text-muted">{t("exams.card.student")}</dt>
        <dd className="font-medium">
          {card.full_name} <span className="font-mono text-muted">{card.student_number}</span>
        </dd>
        <dt className="text-muted">{t("exams.card.father")}</dt>
        <dd>{card.father_name}</dd>
        <dt className="text-muted">{t("exams.card.class")}</dt>
        <dd>
          {card.class_name ?? "—"}
          {card.section_name ? `-${card.section_name}` : ""}
        </dd>
        <dt className="text-muted">{t("exams.card.rollNo")}</dt>
        <dd>{card.roll_no ?? "—"}</dd>
      </dl>
      {card.subjects.length === 0 || held.length === 0 ? (
        <p className="py-6 text-center text-muted">{t("exams.card.empty")}</p>
      ) : (
        <table className="w-full text-xs">
          <thead className="bg-hover">
            <tr>
              <th className="px-2 py-1.5 text-start">{t("exams.subject")}</th>
              {card.assessments.map((a) => (
                <th key={a.id} className={cn("px-2 py-1.5 text-end", !a.held && "text-muted")}>
                  {a.name}
                </th>
              ))}
              <th className="px-2 py-1.5 text-end">{t("exams.card.weighted")}</th>
              <th className="px-2 py-1.5 text-start">{t("exams.results.grade")}</th>
            </tr>
          </thead>
          <tbody>
            {card.subjects.map((s) => (
              <tr key={s.subject_id} className="border-t border-border">
                <td className="px-2 py-1 font-medium">{s.subject_name}</td>
                {s.cells.map((c, i) => (
                  <td
                    key={i}
                    className={cn("px-2 py-1 text-end tabular-nums", c.is_absent && "text-danger")}
                  >
                    {c.max_marks === null
                      ? "—"
                      : c.is_absent
                        ? t("attendance.short.absent")
                        : `${num(c.obtained)}/${num(c.max_marks)}`}
                  </td>
                ))}
                <td className="px-2 py-1 text-end tabular-nums">{pct(s.weighted_percent)}</td>
                <td className="px-2 py-1">{s.grade ?? "—"}</td>
              </tr>
            ))}
            <tr className="border-t-2 border-border font-semibold">
              <td className="px-2 py-1">{t("exams.results.total")}</td>
              {card.totals.map((tt) => (
                <td key={tt.assessment_id} className="px-2 py-1 text-end tabular-nums">
                  {tt.percent === null ? "—" : `${num(tt.total)}/${num(tt.max)}`}
                </td>
              ))}
              <td className="px-2 py-1 text-end tabular-nums">{pct(card.overall_percent)}</td>
              <td className="px-2 py-1">{card.overall_grade ?? "—"}</td>
            </tr>
            <tr className="border-t border-border">
              <td className="px-2 py-1 text-muted">{t("exams.results.percent")}</td>
              {card.totals.map((tt) => (
                <td key={tt.assessment_id} className="px-2 py-1 text-end tabular-nums">
                  {pct(tt.percent)}
                </td>
              ))}
              <td colSpan={2} />
            </tr>
            <tr className="border-t border-border">
              <td className="px-2 py-1 text-muted">{t("exams.results.position")}</td>
              {card.totals.map((tt) => (
                <td key={tt.assessment_id} className="px-2 py-1 text-end tabular-nums">
                  {tt.position === null
                    ? "—"
                    : t("exams.card.position", { position: tt.position, students: tt.students })}
                </td>
              ))}
              <td colSpan={2} />
            </tr>
          </tbody>
        </table>
      )}
      <div className="mt-3 grid grid-cols-2 gap-x-6 gap-y-1 text-xs md:grid-cols-4">
        <span className="text-muted">{t("exams.card.result")}</span>
        <span
          className={cn(
            "font-semibold",
            card.pass === false && "text-danger",
            card.pass && "text-success",
          )}
        >
          {card.pass === null ? "—" : card.pass ? t("exams.card.pass") : t("exams.card.fail")}
        </span>
        <span className="text-muted">{t("exams.card.remarks")}</span>
        <span>{card.overall_remarks ?? "—"}</span>
        <span className="text-muted">{t("exams.card.attendance")}</span>
        <span>
          {card.attendance.total > 0
            ? `${card.attendance.present}/${card.attendance.total} (${card.attendance.percent?.toFixed(1)}%)`
            : "—"}
        </span>
      </div>
      <p className="mt-2 text-2xs text-muted">{t("exams.card.weightsNote", { weights })}</p>
      <div className="mt-8 grid grid-cols-3 gap-6 text-center text-xs text-muted">
        <div className="border-t border-border pt-1">{t("exams.card.signature.teacher")}</div>
        <div className="border-t border-border pt-1">{t("exams.card.signature.principal")}</div>
        <div className="border-t border-border pt-1">{t("exams.card.signature.parent")}</div>
      </div>
    </section>
  );
}
