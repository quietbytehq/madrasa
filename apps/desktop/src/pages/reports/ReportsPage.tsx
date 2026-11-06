import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { Download, FileText, Pencil, Play, Plus, Printer, Trash2 } from "lucide-react";
import { api } from "@/api/endpoints";
import type {
  CustomReport,
  ReportCatalogueEntry,
  ReportDocument,
  ReportLang,
  ReportParamDef,
} from "@/api/types";
import { useSession } from "@/app/session";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";
import { ReportTable } from "./ReportTable";

function thisMonth(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

function monthStart(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;
}

function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** Report library and runner: `/reports` lists, `/reports/:code` runs one. */
export function ReportsPage() {
  const { t, locale } = useI18n();
  const lang: ReportLang = locale === "ur" ? "ur" : "en";
  const { code, id } = useParams();
  const { can } = useSession();
  const navigate = useNavigate();
  const toast = useToast();
  const catalogue = useAsync(() => api.reports(lang), [lang]);
  const custom = useAsync(
    () => (can("reports.read") ? api.customReports() : Promise.resolve([] as CustomReport[])),
    [can],
  );
  const selected = useMemo(
    () => catalogue.data?.flatMap((m) => m.reports).find((r) => r.code === code) ?? null,
    [catalogue.data, code],
  );
  const selectedCustom = useMemo(
    () => custom.data?.find((r) => r.id === id) ?? null,
    [custom.data, id],
  );
  const removeCustom = async (r: CustomReport) => {
    if (!window.confirm(t("reports.custom.deleteConfirm", { name: r.name }))) return;
    try {
      await api.deleteCustomReport(r.id);
      toast.push({ kind: "success", title: t("reports.custom.deleted") });
      await custom.reload();
      if (id === r.id) navigate("/reports");
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    }
  };

  return (
    <>
      <PageHeader
        title={t("reports.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("reports.title") }]}
      />
      <p className="mb-3 text-sm text-muted print:hidden">{t("reports.intro")}</p>
      <div className="grid gap-4 lg:grid-cols-[18rem_1fr]">
        <nav aria-label={t("reports.library")} className="print:hidden">
          <Card title={t("reports.library")} padded={false}>
            {catalogue.loading && !catalogue.data ? (
              <div className="p-4">
                <Spinner />
              </div>
            ) : (
              <ul className="divide-y divide-border">
                {(catalogue.data ?? []).map((m) => (
                  <li key={m.module} className="py-1">
                    <div className="px-3 pt-1 text-2xs font-semibold uppercase tracking-wide text-muted">
                      {m.label}
                    </div>
                    <ul>
                      {m.reports.map((r) => (
                        <li key={r.code}>
                          <Link
                            to={`/reports/${r.code}`}
                            aria-current={r.code === code ? "page" : undefined}
                            className={`block px-3 py-1.5 text-sm hover:bg-hover ${
                              r.code === code ? "bg-selection font-medium text-blue" : "text-text"
                            }`}
                          >
                            <FileText className="me-1 inline h-3.5 w-3.5" aria-hidden />
                            {r.title}
                          </Link>
                        </li>
                      ))}
                    </ul>
                  </li>
                ))}
                {can("reports.read") && (
                  <li className="py-1">
                    <div className="flex items-center justify-between px-3 pt-1">
                      <span className="text-2xs font-semibold uppercase tracking-wide text-muted">
                        {t("reports.custom.title")}
                      </span>
                      <Link
                        to="/reports/custom/new"
                        className="text-xs text-blue hover:underline"
                        aria-label={t("reports.custom.new")}
                      >
                        <Plus className="me-0.5 inline h-3.5 w-3.5" aria-hidden />
                        {t("reports.custom.new")}
                      </Link>
                    </div>
                    {custom.data && custom.data.length === 0 && (
                      <p className="px-3 py-1.5 text-xs text-muted">{t("reports.custom.none")}</p>
                    )}
                    <ul>
                      {(custom.data ?? []).map((r) => (
                        <li key={r.id} className="flex items-center">
                          <Link
                            to={`/reports/custom/${r.id}`}
                            aria-current={r.id === id ? "page" : undefined}
                            className={`block flex-1 px-3 py-1.5 text-sm hover:bg-hover ${
                              r.id === id ? "bg-selection font-medium text-blue" : "text-text"
                            }`}
                          >
                            <FileText className="me-1 inline h-3.5 w-3.5" aria-hidden />
                            {r.name}
                            <span className="ms-1 text-2xs text-muted">
                              {r.is_shared
                                ? t("reports.custom.shared")
                                : t("reports.custom.private")}
                            </span>
                          </Link>
                          {r.can_edit && (
                            <>
                              <Link
                                to={`/reports/custom/${r.id}/edit`}
                                aria-label={`${t("action.edit")}: ${r.name}`}
                                className="p-1 text-muted hover:text-text"
                              >
                                <Pencil className="h-3.5 w-3.5" aria-hidden />
                              </Link>
                              <button
                                type="button"
                                aria-label={`${t("action.delete")}: ${r.name}`}
                                className="p-1 text-muted hover:text-danger"
                                onClick={() => void removeCustom(r)}
                              >
                                <Trash2 className="h-3.5 w-3.5" aria-hidden />
                              </button>
                            </>
                          )}
                        </li>
                      ))}
                    </ul>
                  </li>
                )}
              </ul>
            )}
          </Card>
        </nav>
        <div>
          {selected ? (
            <ReportRunner key={selected.code} def={selected} lang={lang} />
          ) : selectedCustom ? (
            <CustomRunner key={selectedCustom.id} report={selectedCustom} lang={lang} />
          ) : (
            <EmptyState title={t("reports.choose")} />
          )}
        </div>
      </div>
    </>
  );
}

function defaultFor(p: ReportParamDef, activeYearId: string | undefined): string {
  switch (p.kind) {
    case "academic_year":
      return activeYearId ?? "";
    case "month":
      return thisMonth();
    case "date_from":
      return monthStart();
    case "date_to":
    case "date":
      return today();
    case "status":
      return "active";
    case "number":
      return p.name === "threshold" ? "75" : p.name === "top" ? "10" : "1";
    default:
      return "";
  }
}

function ReportRunner({ def, lang }: { def: ReportCatalogueEntry; lang: ReportLang }) {
  const { t } = useI18n();
  const toast = useToast();
  const navigate = useNavigate();
  const [search] = useSearchParams();
  const years = useAsync(() => api.academicYears(), []);
  const classes = useAsync(() => api.classes(), []);
  const [values, setValues] = useState<Record<string, string>>({});
  const [report, setReport] = useState<ReportDocument | null>(null);
  const [busy, setBusy] = useState<"run" | "pdf" | "xlsx" | null>(null);
  const [autoRun, setAutoRun] = useState(search.has("run"));
  const activeYear = years.data?.find((y) => y.is_active)?.id;
  const yearId = values.academic_year_id || activeYear || "";
  const sections = useAsync(
    () =>
      values.class_id
        ? api.sections({ academic_year_id: yearId || undefined, class_id: values.class_id })
        : Promise.resolve([]),
    [values.class_id, yearId],
  );

  const needsAssessment = def.params.some((p) => p.kind === "assessment");
  const assessments = useAsync(
    () => (needsAssessment && yearId ? api.assessments(yearId) : Promise.resolve([])),
    [needsAssessment, yearId],
  );

  // Seed the form once the active year is known: URL query first (dashboard shortcuts),
  // then the report's defaults.
  useEffect(() => {
    setValues((v) => {
      const next = { ...v };
      for (const p of def.params) {
        if (p.name in next) continue;
        next[p.name] = search.get(p.name) ?? defaultFor(p, activeYear);
      }
      if (!next.academic_year_id && activeYear) next.academic_year_id = activeYear;
      return next;
    });
  }, [def, activeYear, search]);

  const set = (name: string, value: string) =>
    setValues((v) =>
      name === "class_id" ? { ...v, class_id: value, section_id: "" } : { ...v, [name]: value },
    );

  const missing = def.params.filter((p) => p.required && !values[p.name]);
  // A shortcut link (`?run`) runs the report as soon as its parameters are complete.
  useEffect(() => {
    if (!autoRun || missing.length > 0 || Object.keys(values).length === 0) return;
    setAutoRun(false);
    setBusy("run");
    api
      .runReport(def.code, values, lang)
      .then(setReport)
      .catch((err: unknown) =>
        toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) }),
      )
      .finally(() => setBusy(null));
  }, [autoRun, missing.length, values, def.code, lang, toast, t]);

  const run = async () => {
    setBusy("run");
    try {
      setReport(await api.runReport(def.code, values, lang));
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  const download = async (format: "pdf" | "xlsx") => {
    setBusy(format);
    try {
      await api.downloadReport(def.code, values, lang, format);
      toast.push({ kind: "success", title: t("export.started") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };

  const control = (p: ReportParamDef) => {
    const value = values[p.name] ?? "";
    switch (p.kind) {
      case "academic_year":
        return (
          <Select
            key={p.name}
            label={p.label}
            value={value}
            onChange={(e) => set(p.name, e.target.value)}
            options={(years.data ?? []).map((y) => ({ value: y.id, label: y.name }))}
          />
        );
      case "class":
        return (
          <Select
            key={p.name}
            label={p.label}
            value={value}
            onChange={(e) => set(p.name, e.target.value)}
            options={[
              { value: "", label: t("reports.allClasses") },
              ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
            ]}
          />
        );
      case "section":
        return (
          <Select
            key={p.name}
            label={p.label}
            value={value}
            disabled={!values.class_id}
            onChange={(e) => set(p.name, e.target.value)}
            options={[
              { value: "", label: t("reports.allSections") },
              ...(sections.data ?? []).map((s) => ({ value: s.id, label: s.name })),
            ]}
          />
        );
      case "status":
        return (
          <Select
            key={p.name}
            label={p.label}
            value={value}
            onChange={(e) => set(p.name, e.target.value)}
            options={(["active", "left", "graduated"] as const).map((s) => ({
              value: s,
              label: t(`reports.status.${s}`),
            }))}
          />
        );
      case "month":
        return (
          <Input
            key={p.name}
            label={p.label}
            type="month"
            value={value}
            required={p.required}
            onChange={(e) => set(p.name, e.target.value)}
          />
        );
      case "date_from":
      case "date_to":
      case "date":
        return (
          <Input
            key={p.name}
            label={p.label}
            type="date"
            value={value}
            required={p.required}
            onChange={(e) => set(p.name, e.target.value)}
          />
        );
      case "assessment":
        return (
          <Select
            key={p.name}
            label={p.label}
            value={value}
            onChange={(e) => set(p.name, e.target.value)}
            placeholder={t("reports.pickAssessment")}
            options={(assessments.data ?? []).map((a) => ({ value: a.id, label: a.name }))}
          />
        );
      case "number":
        return (
          <Input
            key={p.name}
            label={p.label}
            type="number"
            min={0}
            max={1000}
            value={value}
            required={p.required}
            onChange={(e) => set(p.name, e.target.value)}
          />
        );
      case "student_number":
        return (
          <Input
            key={p.name}
            label={p.label}
            value={value}
            required={p.required}
            placeholder="KHI-MC-2026-00001"
            onChange={(e) => set(p.name, e.target.value)}
          />
        );
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <Card
        title={def.title}
        className="print:hidden"
        actions={
          <Button size="sm" variant="ghost" onClick={() => navigate("/reports")}>
            {t("action.back")}
          </Button>
        }
      >
        <p className="mb-3 text-sm text-muted">{def.description}</p>
        <form
          className="flex flex-wrap items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void run();
          }}
        >
          {def.params.map((p) => (
            <div key={p.name} className="w-44">
              {control(p)}
            </div>
          ))}
          <Button
            type="submit"
            loading={busy === "run"}
            disabled={missing.length > 0}
            icon={<Play className="h-4 w-4" aria-hidden />}
          >
            {t("reports.run")}
          </Button>
          <Button
            type="button"
            variant="secondary"
            loading={busy === "pdf"}
            disabled={missing.length > 0}
            icon={<Download className="h-4 w-4" aria-hidden />}
            onClick={() => void download("pdf")}
          >
            {t("reports.pdf")}
          </Button>
          <Button
            type="button"
            variant="secondary"
            loading={busy === "xlsx"}
            disabled={missing.length > 0}
            icon={<Download className="h-4 w-4" aria-hidden />}
            onClick={() => void download("xlsx")}
          >
            {t("reports.excel")}
          </Button>
          {report && (
            <Button
              type="button"
              variant="ghost"
              icon={<Printer className="h-4 w-4" aria-hidden />}
              onClick={() => window.print()}
            >
              {t("reports.print")}
            </Button>
          )}
        </form>
      </Card>
      {report && (
        <Card>
          <div className="mb-2 text-xs text-muted print:hidden">
            {t("reports.rows", { count: report.row_count })}
          </div>
          {report.row_count === 0 && (
            <p className="mb-2 text-sm text-muted print:hidden">{t("reports.empty")}</p>
          )}
          <ReportTable report={report} generatedLabel={t("reports.generated")} />
        </Card>
      )}
    </div>
  );
}

/** Runs a saved custom report: no parameters beyond the language. */
function CustomRunner({ report: saved, lang }: { report: CustomReport; lang: ReportLang }) {
  const { t } = useI18n();
  const toast = useToast();
  const navigate = useNavigate();
  const [busy, setBusy] = useState<"pdf" | "xlsx" | null>(null);
  const report = useAsync(() => api.runCustomReport(saved.id, lang), [saved.id, lang]);
  const download = async (format: "pdf" | "xlsx") => {
    setBusy(format);
    try {
      await api.downloadCustomReport(saved.id, lang, format);
      toast.push({ kind: "success", title: t("export.started") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  return (
    <div className="flex flex-col gap-3">
      <Card
        title={saved.name}
        className="print:hidden"
        actions={
          <div className="flex flex-wrap gap-2">
            {saved.can_edit && (
              <Button
                size="sm"
                variant="secondary"
                onClick={() => navigate(`/reports/custom/${saved.id}/edit`)}
              >
                {t("action.edit")}
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={() => navigate("/reports")}>
              {t("action.back")}
            </Button>
          </div>
        }
      >
        {saved.description && <p className="mb-3 text-sm text-muted">{saved.description}</p>}
        <p className="mb-3 text-xs text-muted">
          {saved.is_shared ? t("reports.custom.shared") : t("reports.custom.private")}
          {saved.owner_name ? ` · ${t("reports.custom.by", { name: saved.owner_name })}` : ""}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            loading={busy === "pdf"}
            icon={<Download className="h-4 w-4" aria-hidden />}
            onClick={() => void download("pdf")}
          >
            {t("reports.pdf")}
          </Button>
          <Button
            variant="secondary"
            loading={busy === "xlsx"}
            icon={<Download className="h-4 w-4" aria-hidden />}
            onClick={() => void download("xlsx")}
          >
            {t("reports.excel")}
          </Button>
          {report.data && (
            <Button
              variant="ghost"
              icon={<Printer className="h-4 w-4" aria-hidden />}
              onClick={() => window.print()}
            >
              {t("reports.print")}
            </Button>
          )}
        </div>
      </Card>
      {report.loading && !report.data ? (
        <Spinner />
      ) : report.data ? (
        <Card>
          <div className="mb-2 text-xs text-muted print:hidden">
            {t("reports.rows", { count: report.data.row_count })}
          </div>
          {report.data.row_count === 0 && (
            <p className="mb-2 text-sm text-muted print:hidden">{t("reports.empty")}</p>
          )}
          <ReportTable report={report.data} generatedLabel={t("reports.generated")} />
        </Card>
      ) : (
        <EmptyState title={errorMessage(report.error, t("app.error.generic"))} />
      )}
    </div>
  );
}
