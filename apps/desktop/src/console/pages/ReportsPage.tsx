import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { useAsync } from "@/lib/useAsync";
import { hub } from "../api";
import type { ColumnKind, HubReport, ReportParams } from "../api";
import { useFail } from "../session";
import { cell, td, th } from "../format";
import { PageTitle } from "./common";

const NUMERIC: ColumnKind[] = ["int", "money", "percent"];

/** Network-wide reports: pick one, a period and a school; read it here or download it. */
export function ReportsPage() {
  const i18n = useI18n();
  const { t, locale } = i18n;
  const fail = useFail();
  const catalogue = useAsync(() => hub.reports(locale), [locale]);
  const schools = useAsync(() => hub.schools(), []);
  const [code, setCode] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [school, setSchool] = useState("");
  const [report, setReport] = useState<HubReport | null>(null);
  const [busy, setBusy] = useState<"run" | "csv" | "xlsx" | null>(null);

  const chosen = catalogue.data?.find((r) => r.code === code) ?? null;
  const params = (): ReportParams => ({
    lang: locale,
    ...(chosen?.takes_period && from ? { from } : {}),
    ...(chosen?.takes_period && to ? { to } : {}),
    ...(school ? { school_id: school } : {}),
  });

  const run = async () => {
    setBusy("run");
    try {
      setReport(await hub.report(code, params()));
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };
  const save = async (format: "csv" | "xlsx") => {
    setBusy(format);
    try {
      await hub.downloadReport(code, params(), format);
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <PageTitle title={t("hub.nav.reports")} about={t("hub.reports.about")} />
      <Card>
        {catalogue.loading && !catalogue.data ? (
          <Spinner />
        ) : (
          <div className="flex flex-col gap-3">
            <div className="grid gap-3 md:grid-cols-4">
              <Select
                label={t("hub.reports.report")}
                value={code}
                onChange={(e) => {
                  setCode(e.target.value);
                  setReport(null);
                }}
                placeholder={t("hub.reports.choose")}
                options={(catalogue.data ?? []).map((r) => ({ value: r.code, label: r.title }))}
              />
              <Select
                label={t("hub.schools.school")}
                value={school}
                onChange={(e) => setSchool(e.target.value)}
                options={[
                  { value: "", label: t("hub.reports.allSchools") },
                  ...(schools.data ?? [])
                    .filter((s) => s.status === "active")
                    .map((s) => ({ value: s.id, label: `${s.name} (${s.code})` })),
                ]}
              />
              <Input
                label={t("hub.reports.from")}
                type="date"
                value={from}
                onChange={(e) => setFrom(e.target.value)}
                disabled={!chosen?.takes_period}
              />
              <Input
                label={t("hub.reports.to")}
                type="date"
                value={to}
                onChange={(e) => setTo(e.target.value)}
                disabled={!chosen?.takes_period}
              />
            </div>
            {chosen && <p className="text-xs text-muted">{chosen.about}</p>}
            <div className="flex flex-wrap gap-2">
              <Button loading={busy === "run"} disabled={!code} onClick={() => void run()}>
                {t("hub.reports.run")}
              </Button>
              <Button
                variant="secondary"
                loading={busy === "xlsx"}
                disabled={!code}
                onClick={() => void save("xlsx")}
              >
                {t("hub.reports.excel")}
              </Button>
              <Button
                variant="secondary"
                loading={busy === "csv"}
                disabled={!code}
                onClick={() => void save("csv")}
              >
                {t("hub.reports.csv")}
              </Button>
            </div>
          </div>
        )}
      </Card>

      {report && (
        <Card
          className="mt-4"
          padded={false}
          title={report.title}
          footer={
            <>
              {t("hub.reports.footer", {
                schools: report.schools,
                at: i18n.formatDate(report.generated_at, {
                  dateStyle: "medium",
                  timeStyle: "short",
                }),
              })}
              {report.notes.map((n) => (
                <div key={n}>{n}</div>
              ))}
            </>
          }
        >
          {report.rows.length === 0 ? (
            <p className="p-4 text-sm text-muted">{t("hub.reports.empty")}</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead className="border-b border-border">
                  <tr>
                    {report.columns.map((c) => (
                      <th
                        key={c.key}
                        className={cn(th, NUMERIC.includes(c.kind) && "text-end")}
                        scope="col"
                      >
                        {c.label}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {report.rows.map((row, i) => (
                    <tr key={i} className="border-t border-border">
                      {report.columns.map((c, j) => (
                        <td
                          key={c.key}
                          className={cn(td, NUMERIC.includes(c.kind) && "text-end tabular-nums")}
                        >
                          {cell(row[j], c.kind, i18n)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
                {report.totals.length > 0 && (
                  <tfoot>
                    {report.totals.map((row, i) => (
                      <tr key={i} className="border-t border-border bg-hover font-semibold">
                        {report.columns.map((c, j) => (
                          <td
                            key={c.key}
                            className={cn(td, NUMERIC.includes(c.kind) && "text-end tabular-nums")}
                          >
                            {row[j] === null || row[j] === undefined || row[j] === ""
                              ? ""
                              : cell(row[j], c.kind, i18n)}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tfoot>
                )}
              </table>
            </div>
          )}
        </Card>
      )}
    </>
  );
}
