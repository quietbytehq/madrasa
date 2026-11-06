import type { ReportDocument } from "@/api/types";
import { cn } from "@/lib/cn";

/**
 * On-screen (and print) rendering of a report document: school and title header, the
 * parameters it was run with, one table with group headings, subtotals and the total.
 */
export function ReportTable({
  report,
  generatedLabel,
}: {
  report: ReportDocument;
  generatedLabel: string;
}) {
  const width = report.columns.length;
  const alignClass = (i: number) =>
    report.columns[i]?.align === "end"
      ? "text-end"
      : report.columns[i]?.align === "center"
        ? "text-center"
        : "text-start";
  return (
    <div
      dir={report.lang === "ur" ? "rtl" : "ltr"}
      lang={report.lang}
      className={cn("report-print bg-card text-text", report.lang === "ur" && "font-urdu")}
    >
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <div>
          <div className="text-base font-semibold text-navy">{report.school_name}</div>
          <h2 className="text-lg font-semibold">{report.title}</h2>
          {report.subtitle && <div className="text-sm text-muted">{report.subtitle}</div>}
        </div>
        <div className="text-xs text-muted">
          {generatedLabel}: {report.generated_on}
        </div>
      </div>
      {report.params.length > 0 && (
        <dl className="mb-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
          {report.params.map((p) => (
            <div key={p.label} className="flex gap-1">
              <dt>{p.label}:</dt>
              <dd className="font-medium text-text">{p.value}</dd>
            </div>
          ))}
        </dl>
      )}
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="bg-hover text-xs uppercase tracking-wide text-muted">
            {report.columns.map((c, i) => (
              <th key={i} className={cn("border border-border px-2 py-1", alignClass(i))}>
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {report.groups.map((g, gi) => (
            <GroupRows key={gi} g={g} width={width} alignClass={alignClass} />
          ))}
          {report.total && (
            <tr className="bg-hover font-semibold">
              {report.total.map((v, i) => (
                <td key={i} className={cn("border border-border px-2 py-1", alignClass(i))}>
                  {v}
                </td>
              ))}
            </tr>
          )}
        </tbody>
      </table>
      {report.notes.map((n) => (
        <p key={n} className="mt-2 text-xs text-muted">
          {n}
        </p>
      ))}
    </div>
  );
}

function GroupRows({
  g,
  width,
  alignClass,
}: {
  g: ReportDocument["groups"][number];
  width: number;
  alignClass: (i: number) => string;
}) {
  return (
    <>
      {g.heading && (
        <tr className="bg-bg">
          <th
            colSpan={width}
            className="border border-border px-2 py-1 text-start text-sm font-semibold"
          >
            {g.heading}
          </th>
        </tr>
      )}
      {g.rows.map((row, ri) => (
        <tr key={ri} className="odd:bg-card even:bg-bg/60">
          {row.map((v, i) => (
            <td key={i} className={cn("border border-border px-2 py-1", alignClass(i))}>
              {v}
            </td>
          ))}
        </tr>
      ))}
      {g.subtotal && (
        <tr className="bg-bg font-medium">
          {g.subtotal.map((v, i) => (
            <td key={i} className={cn("border border-border px-2 py-1", alignClass(i))}>
              {v}
            </td>
          ))}
        </tr>
      )}
    </>
  );
}
