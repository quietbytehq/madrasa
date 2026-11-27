import { useEffect, useMemo, useRef, useState } from "react";
import type { ChangeEvent } from "react";
import { useNavigate } from "react-router-dom";
import { Download, FileSpreadsheet, Stethoscope, Upload } from "lucide-react";
import { api } from "@/api/endpoints";
import type {
  DryRun,
  DuplicatePolicy,
  ImportBatch,
  ImportField,
  ImportKind,
  ImportPreview,
  ImportRequest,
  ImportResult,
  ParsedFile,
  RowOutcome,
} from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useHasFeature } from "@/app/features";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { pickSheet } from "@/lib/sheets";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { importDraft } from "./importDraft";
import { discardImport, keepImport, keptImport } from "./importStore";
import type { KeptImport } from "./importStore";

const KINDS: ImportKind[] = [
  "students",
  "guardians",
  "staff",
  "fee_structure",
  "opening_balances",
  "enquiries",
];
const STEPS: TranslationKey[] = [
  "import.step.type",
  "import.step.file",
  "import.step.mapping",
  "import.step.preview",
  "import.step.done",
];

function readAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error);
    reader.onload = () => {
      const result = String(reader.result ?? "");
      resolve(result.includes(",") ? result.slice(result.indexOf(",") + 1) : result);
    };
    reader.readAsDataURL(file);
  });
}

/** CSV of the rows that have errors, with a "Problems" column, for fixing in Excel. */
export function errorCsv(headers: string[], rows: string[][], outcomes: RowOutcome[]): string {
  const esc = (v: string) => (/[",\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);
  const lines = [[...headers, "Problems"].map(esc).join(",")];
  for (const o of outcomes) {
    if (o.action !== "error") continue;
    const raw = rows[o.row_no - 2] ?? [];
    const problems = o.errors.map((e) => `${e.field}: ${e.message}`).join("; ");
    lines.push([...headers.map((_, i) => raw[i] ?? ""), problems].map(esc).join(","));
  }
  return String.fromCharCode(0xfeff) + lines.join("\n");
}

export function ImportPage() {
  const { t, formatDate } = useI18n();
  const { can, user } = useSession();
  const toast = useToast();
  // An import in progress survives a visit to another page (state lives in importDraft),
  // and closing the app (the file, the columns and the corrections are kept by importStore).
  const saved = importDraft.get();
  const [step, setStep] = useState(saved.step);
  const [kind, setKind] = useState<ImportKind>(saved.kind);
  const [parsed, setParsed] = useState<ParsedFile | null>(saved.parsed);
  /** A workbook with several sheets waits here until the user confirms which one to read. */
  const [pending, setPending] = useState<{ file: File; parsed: ParsedFile; sheet: string } | null>(
    null,
  );
  const [mapping, setMapping] = useState<Record<string, number>>(saved.mapping);
  const [remember, setRemember] = useState(saved.remember);
  const [policy, setPolicy] = useState<DuplicatePolicy>(saved.policy);
  const [preview, setPreview] = useState<ImportPreview | null>(saved.preview);
  const [edited, setEdited] = useState<string[][] | null>(saved.edited);
  const [onlyProblems, setOnlyProblems] = useState(false);
  const [result, setResult] = useState<ImportResult | null>(saved.result);
  const [startedAt, setStartedAt] = useState<string | null>(saved.startedAt);
  const [confirmRollback, setConfirmRollback] = useState<ImportBatch | null>(null);
  useEffect(() => {
    importDraft.set({
      step,
      kind,
      parsed,
      mapping,
      remember,
      policy,
      preview,
      edited,
      result,
      startedAt,
    });
  }, [step, kind, parsed, mapping, remember, policy, preview, edited, result, startedAt]);
  const [busy, setBusy] = useState(false);

  // ---- an import left unfinished when the app was last closed --------------------------
  const userId = user?.id ?? null;
  /** What was found on this PC when the page opened: undefined while still looking. */
  const [offer, setOffer] = useState<KeptImport | null | undefined>(
    saved.parsed ? null : undefined,
  );
  /** Whether the import on screen is kept on this PC: null before the first save. */
  const [kept, setKept] = useState<boolean | null>(null);
  useEffect(() => {
    if (offer !== undefined) return;
    if (!userId) return setOffer(null);
    let alive = true;
    void keptImport(userId).then((k) => alive && setOffer(k));
    return () => {
      alive = false;
    };
  }, [offer, userId]);
  // Keep it a moment after every change; what is waiting is written at once when the page
  // is left, so the last correction is not lost to the delay.
  const waiting = useRef<KeptImport | null>(null);
  const flush = useRef(() => {});
  flush.current = () => {
    const k = waiting.current;
    waiting.current = null;
    if (k) void keepImport(k).then(setKept);
  };
  useEffect(() => {
    if (!userId || !parsed || !startedAt || (step !== 2 && step !== 3)) return;
    waiting.current = {
      user: userId,
      at: startedAt,
      step,
      kind,
      parsed,
      mapping,
      remember,
      policy,
      edited,
    };
    const timer = setTimeout(() => flush.current(), 400);
    return () => clearTimeout(timer);
  }, [userId, step, kind, parsed, mapping, remember, policy, edited, startedAt]);
  useEffect(() => () => flush.current(), []);
  const forget = () => {
    waiting.current = null;
    setKept(null);
    if (userId) void discardImport(userId);
  };
  const fields = useAsync(() => api.importFields(kind), [kind]);
  const batches = useAsync(() => api.importBatches(), []);
  const navigate = useNavigate();
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const rows = edited ?? parsed?.rows ?? [];

  const has = useHasFeature();
  const allowed = (k: ImportKind) =>
    k === "enquiries"
      ? has("admissions") && can("admissions.manage")
      : can(
          k === "staff"
            ? "teachers.manage"
            : k === "fee_structure"
              ? "fees.manage"
              : "students.manage",
        );

  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setBusy(true);
    try {
      const content_base64 = await readAsBase64(file);
      const p = await api.parseImport({ kind, file_name: file.name, content_base64 });
      if (p.sheets.length > 1) {
        setPending({ file, parsed: p, sheet: pickSheet(kind, p.sheets) ?? p.sheet });
        return;
      }
      accept(p);
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
      e.target.value = "";
    }
  };

  const accept = (p: ParsedFile) => {
    setPending(null);
    setParsed(p);
    setMapping(p.mapping);
    setEdited(null);
    setPreview(null);
    setStartedAt(new Date().toISOString());
    setStep(2);
  };

  /** Take up the import that was left unfinished: as it was, and checked again if it had been. */
  const continueKept = async (k: KeptImport) => {
    setOffer(null);
    setKind(k.kind);
    setParsed(k.parsed);
    setMapping(k.mapping);
    setRemember(k.remember);
    setPolicy(k.policy);
    setEdited(k.edited);
    setPreview(null);
    setResult(null);
    setStartedAt(k.at);
    setStep(2);
    if (k.step !== 3) return;
    // The records may have changed since: the check is made again, not remembered.
    setBusy(true);
    try {
      setPreview(
        await api.previewImport({
          kind: k.kind,
          file_name: k.parsed.file_name,
          headers: k.parsed.headers,
          rows: k.edited ?? k.parsed.rows,
          mapping: k.mapping,
          options: { duplicates: k.policy },
          save_mapping: k.remember,
        }),
      );
      setStep(3);
      toast.push({ kind: "info", title: t("import.resume.rechecked") });
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };

  const discardKept = () => {
    setOffer(null);
    forget();
    toast.push({ kind: "success", title: t("import.resume.discarded") });
  };

  const confirmSheet = async () => {
    if (!pending) return;
    if (pending.sheet === pending.parsed.sheet) {
      accept(pending.parsed);
      return;
    }
    setBusy(true);
    try {
      const content_base64 = await readAsBase64(pending.file);
      accept(
        await api.parseImport({
          kind,
          file_name: pending.file.name,
          content_base64,
          sheet: pending.sheet,
        }),
      );
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };

  const request = (): ImportRequest | null =>
    parsed
      ? {
          kind,
          file_name: parsed.file_name,
          headers: parsed.headers,
          rows,
          mapping,
          options: { duplicates: policy },
          save_mapping: remember,
        }
      : null;

  const runPreview = async () => {
    const req = request();
    if (!req) return;
    setBusy(true);
    try {
      setPreview(await api.previewImport(req));
      setStep(3);
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };

  const applyNow = async () => {
    const req = request();
    if (!req) return;
    setBusy(true);
    try {
      const r = await api.applyImport(req);
      setResult(r);
      setStep(4);
      // Confirmed: it is in the school's records now, and no longer an unfinished import.
      forget();
      void batches.reload();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };

  const [confirmApply, setConfirmApply] = useState(false);

  const editCell = (rowNo: number, col: number, value: string) => {
    const base = (edited ?? parsed?.rows ?? []).map((r) => [...r]);
    const idx = rowNo - 2;
    while ((base[idx] ?? []).length <= col) base[idx]!.push("");
    base[idx]![col] = value;
    setEdited(base);
  };

  const downloadErrors = () => {
    if (!parsed || !preview) return;
    const blob = new Blob([errorCsv(parsed.headers, rows, preview.rows)], {
      type: "text/csv;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `errors-${parsed.file_name.replace(/\.[^.]+$/, "")}.csv`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  const reset = () => {
    importDraft.clear();
    forget();
    setStartedAt(null);
    setStep(0);
    setParsed(null);
    setMapping({});
    setPreview(null);
    setEdited(null);
    setResult(null);
  };

  // Mirrors `missing_required` in the backend: first + last name stand in for the full name,
  // a parent/guardian name for the father's name.
  const requiredMissing = useMemo(
    () =>
      (fields.data ?? []).filter((f) => {
        if (!f.required || mapping[f.key] !== undefined) return false;
        if (f.key === "full_name")
          return mapping.first_name === undefined || mapping.last_name === undefined;
        if (f.key === "father_name") return mapping.guardian_name === undefined;
        return true;
      }),
    [fields.data, mapping],
  );
  // Required fields that are not mapped but derived from other columns (v0.2.0-alpha B4):
  // shown as "from First name + Last name" / "from Guardian 1" instead of missing.
  const derived = useMemo(() => {
    const out: Record<string, "full_name" | "father_name"> = {};
    if (
      mapping.full_name === undefined &&
      mapping.first_name !== undefined &&
      mapping.last_name !== undefined
    )
      out.full_name = "full_name";
    if (mapping.father_name === undefined && mapping.guardian_name !== undefined)
      out.father_name = "father_name";
    return out;
  }, [mapping]);

  return (
    <>
      <PageHeader
        title={t("import.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("import.title") }]}
        actions={
          (can("students.manage") || can("settings.manage")) && (
            <Button
              variant="secondary"
              icon={<Stethoscope className="h-4 w-4" aria-hidden />}
              onClick={() => navigate("/data-health")}
            >
              {t("health.title")}
            </Button>
          )
        }
      />
      <p className="mb-3 text-sm text-muted">{t("import.subtitle")}</p>
      <ol className="mb-4 flex flex-wrap gap-2 text-xs" aria-label={t("import.title")}>
        {STEPS.map((key, i) => (
          <li
            key={key}
            aria-current={step === i ? "step" : undefined}
            className={cn(
              "rounded-full border px-2.5 py-1",
              step === i
                ? "border-blue bg-selection text-blue"
                : i < step
                  ? "border-success text-success"
                  : "border-border text-muted",
            )}
          >
            {i + 1}. {t(key)}
          </li>
        ))}
      </ol>
      {(step === 2 || step === 3) && kept !== null && (
        <p
          role={kept ? undefined : "status"}
          className={cn("mb-3 text-xs", kept ? "text-muted" : "text-danger")}
          data-testid="import-kept"
        >
          {t(kept ? "import.resume.kept" : "import.resume.notKept")}
        </p>
      )}

      {step === 0 && offer && (
        <Card title={t("import.resume.title")}>
          <div className="flex flex-col gap-3" data-testid="import-resume">
            <p className="text-sm text-text">
              {t("import.resume.body", {
                kind: t(`import.kind.${offer.kind}`),
                name: offer.parsed.file_name,
                rows: (offer.edited ?? offer.parsed.rows).length,
                date: formatDate(offer.at),
              })}
            </p>
            <p className="text-xs text-muted">{t("import.resume.nothingSaved")}</p>
            <div className="flex flex-wrap gap-2">
              <Button loading={busy} onClick={() => void continueKept(offer)} autoFocus>
                {t("import.resume.continue", { date: formatDate(offer.at) })}
              </Button>
              <Button variant="secondary" disabled={busy} onClick={discardKept}>
                {t("import.resume.discard")}
              </Button>
            </div>
          </div>
        </Card>
      )}

      {step === 0 && offer === null && (
        <div className="grid gap-3 md:grid-cols-3">
          {KINDS.map((k) => (
            <button
              key={k}
              type="button"
              disabled={!allowed(k)}
              onClick={() => {
                setKind(k);
                setStep(1);
              }}
              className={cn(
                "rounded border bg-card p-4 text-start shadow-sm hover:border-blue disabled:opacity-50",
                kind === k ? "border-blue" : "border-border",
              )}
            >
              <div className="flex items-center gap-2 font-medium text-navy">
                <FileSpreadsheet className="h-4 w-4" aria-hidden />
                {t(`import.kind.${k}`)}
              </div>
              <p className="mt-1 text-xs text-muted">{t(`import.kind.${k}.hint`)}</p>
            </button>
          ))}
        </div>
      )}

      {step === 1 && pending && (
        <Card title={t("import.sheet.title")}>
          <div className="flex flex-col gap-3">
            <p className="text-sm text-muted">{t("import.sheet.hint")}</p>
            <Select
              label={t("import.file.sheet")}
              value={pending.sheet}
              onChange={(e) => setPending({ ...pending, sheet: e.target.value })}
              options={pending.parsed.sheets.map((name) => ({
                value: name,
                label:
                  name === pending.parsed.sheet
                    ? `${name} (${t("import.sheet.rows", { rows: pending.parsed.rows.length })})`
                    : name,
              }))}
              className="max-w-xs"
              autoFocus
            />
            <div className="flex gap-2">
              <Button onClick={() => void confirmSheet()} loading={busy}>
                {t("action.continue")}
              </Button>
              <Button variant="ghost" onClick={() => setPending(null)}>
                {t("action.back")}
              </Button>
            </div>
          </div>
        </Card>
      )}

      {step === 1 && !pending && (
        <Card title={t(`import.kind.${kind}`)}>
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-sm text-muted">{t("import.template")}:</span>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void api.downloadTemplate(kind, "xlsx").catch(fail)}
              >
                {t("import.template.xlsx")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void api.downloadTemplate(kind, "csv").catch(fail)}
              >
                {t("import.template.csv")}
              </Button>
            </div>
            <label className="flex cursor-pointer flex-col items-center gap-2 rounded border border-dashed border-border p-8 text-sm hover:border-blue">
              <Upload className="h-6 w-6 text-muted" aria-hidden />
              <span className="font-medium">
                {busy ? t("import.file.reading") : t("import.file.choose")}
              </span>
              <span className="text-xs text-muted">{t("import.file.hint")}</span>
              <input
                type="file"
                accept=".xlsx,.xlsm,.xls,.csv,.tsv,.txt"
                className="sr-only"
                aria-label={t("import.file.choose")}
                disabled={busy}
                onChange={(e) => void onFile(e)}
              />
            </label>
            <div>
              <Button variant="ghost" onClick={() => setStep(0)}>
                {t("action.back")}
              </Button>
            </div>
          </div>
        </Card>
      )}

      {step === 2 && parsed && (
        <Card
          title={t("import.file.rows", { rows: parsed.rows.length, name: parsed.file_name })}
          actions={
            <Button
              loading={busy}
              disabled={requiredMissing.length > 0}
              onClick={() => void runPreview()}
            >
              {t("import.preview.run")}
            </Button>
          }
        >
          <p className="mb-2 text-xs text-muted">
            {parsed.mapping_saved ? t("import.mapping.saved") : t("import.mapping.hint")}
          </p>
          <MappingTable
            fields={fields.data ?? []}
            headers={parsed.headers}
            sample={parsed.rows[0] ?? []}
            mapping={mapping}
            derived={derived}
            onChange={setMapping}
            t={t}
          />
          <div className="mt-3 flex flex-wrap items-end gap-4">
            <Select
              label={t("import.duplicates")}
              value={policy}
              onChange={(e) => setPolicy(e.target.value as DuplicatePolicy)}
              options={(["skip", "update", "merge"] as const).map((p) => ({
                value: p,
                label: t(`import.duplicates.${p}`),
              }))}
              className="w-80"
            />
            <label className="flex items-center gap-2 pb-2 text-sm">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              {t("import.mapping.remember")}
            </label>
            <span className="flex-1" />
            <Button variant="ghost" onClick={() => setStep(1)}>
              {t("action.back")}
            </Button>
          </div>
        </Card>
      )}

      {step === 3 && parsed && preview && (
        <Card
          title={t("import.preview.summary", { ...preview.summary })}
          actions={
            <div className="flex flex-wrap gap-2">
              <Button variant="ghost" onClick={() => setStep(2)}>
                {t("action.back")}
              </Button>
              {preview.summary.error > 0 && (
                <Button variant="secondary" onClick={downloadErrors}>
                  {t("import.preview.downloadErrors")}
                </Button>
              )}
              <Button variant="secondary" loading={busy} onClick={() => void runPreview()}>
                {t("import.preview.run")}
              </Button>
              <Button
                loading={busy}
                disabled={preview.summary.create + preview.summary.update === 0}
                onClick={() => setConfirmApply(true)}
              >
                {t("import.apply", { count: preview.summary.create + preview.summary.update })}
              </Button>
            </div>
          }
          padded={false}
        >
          <DryRunPanel
            preview={preview}
            onDownload={() => {
              const req = request();
              if (req) void api.downloadImportReport(req).catch(fail);
            }}
          />
          {preview.summary.error > 0 && (
            <p
              role="alert"
              className="border-b border-border bg-warning-bg px-4 py-2 text-xs text-text"
            >
              {t("import.preview.errorsHint")}
            </p>
          )}
          <div className="flex items-center gap-2 px-4 py-2 text-xs">
            {preview.unmapped.length > 0 && (
              <p className="rounded border border-warning/40 bg-warning-bg px-2 py-1 text-xs text-text">
                {t("import.preview.unmapped", { columns: preview.unmapped.join(", ") })}
              </p>
            )}
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={onlyProblems}
                onChange={(e) => setOnlyProblems(e.target.checked)}
              />
              {t("import.preview.showErrors")}
            </label>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-xs">
              <thead className="bg-hover text-muted">
                <tr>
                  <th className="px-2 py-1.5 text-start">{t("import.preview.row")}</th>
                  <th className="px-2 py-1.5 text-start">{t("import.preview.action")}</th>
                  {parsed.headers.map((h, i) => (
                    <th key={i} className="px-2 py-1.5 text-start">
                      {h}
                    </th>
                  ))}
                  <th className="px-2 py-1.5 text-start">{t("import.preview.problems")}</th>
                </tr>
              </thead>
              <tbody>
                {preview.rows
                  .filter((o) => !onlyProblems || o.action === "error" || o.warnings.length > 0)
                  .map((o) => {
                    const raw = rows[o.row_no - 2] ?? [];
                    const badCols = new Set(
                      o.errors
                        .map((e) => mapping[e.field])
                        .filter((c): c is number => c !== undefined),
                    );
                    return (
                      <tr
                        key={o.row_no}
                        data-testid={`preview-row-${o.row_no}`}
                        className={cn(
                          "border-t border-border",
                          o.action === "error" && "bg-danger-bg/30",
                        )}
                      >
                        <td className="px-2 py-1 tabular-nums text-muted">{o.row_no}</td>
                        <td className="px-2 py-1">
                          <ActionBadge action={o.action} t={t} />
                          {o.matched && <div className="text-2xs text-muted">{o.matched}</div>}
                        </td>
                        {parsed.headers.map((_, i) => (
                          <td key={i} className="px-1 py-0.5">
                            {o.action === "error" ? (
                              <input
                                aria-label={`${t("import.preview.row")} ${o.row_no}: ${parsed.headers[i]}`}
                                value={raw[i] ?? ""}
                                onChange={(e) => editCell(o.row_no, i, e.target.value)}
                                className={cn(
                                  "h-6 w-full min-w-24 rounded border bg-card px-1",
                                  badCols.has(i) ? "border-danger" : "border-border",
                                )}
                              />
                            ) : (
                              <span className="whitespace-nowrap">{raw[i] ?? ""}</span>
                            )}
                          </td>
                        ))}
                        <td className="px-2 py-1">
                          {o.errors.map((e, i) => (
                            <div key={i} className="text-danger">
                              {e.message}
                            </div>
                          ))}
                          {o.warnings.map((w, i) => (
                            <div key={i} className="text-warning">
                              {w.message}
                            </div>
                          ))}
                        </td>
                      </tr>
                    );
                  })}
              </tbody>
            </table>
          </div>
        </Card>
      )}

      {step === 4 && result && (
        <Card title={t("import.step.done")}>
          <p role="status" className="text-sm">
            {t("import.apply.done", {
              created: result.batch.created_rows,
              updated: result.batch.updated_rows,
              skipped: result.batch.skipped_rows,
            })}
          </p>
          <p className="mt-1 text-xs text-muted">{t("import.apply.rollbackHint")}</p>
          <div className="mt-3">
            <Button onClick={reset}>{t("import.startOver")}</Button>
          </div>
        </Card>
      )}

      <Card title={t("import.history")} padded={false} className="mt-4">
        {batches.loading && !batches.data ? (
          <div className="flex justify-center py-6">
            <Spinner />
          </div>
        ) : !batches.data || batches.data.length === 0 ? (
          <EmptyState title={t("import.history.empty")} />
        ) : (
          <ul className="divide-y divide-border text-sm">
            {batches.data.map((b: ImportBatch) => (
              <li key={b.id} className="flex flex-wrap items-center gap-3 px-4 py-2">
                <span className="text-muted">{formatDate(b.applied_at)}</span>
                <Badge tone="lavender">{t(`import.kind.${b.kind}`)}</Badge>
                <span className="font-mono text-xs">{b.file_name}</span>
                <span className="text-xs text-muted">
                  {t("import.history.rows", {
                    created: b.created_rows,
                    updated: b.updated_rows,
                    skipped: b.skipped_rows,
                  })}
                </span>
                <span className="flex-1" />
                {b.status === "rolled_back" ? (
                  <Badge tone="neutral">{t("import.history.rolledBack")}</Badge>
                ) : allowed(b.kind) ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-danger"
                    onClick={() => setConfirmRollback(b)}
                  >
                    {t("import.history.rollback")}
                  </Button>
                ) : (
                  <Badge tone="success">{t("import.history.applied")}</Badge>
                )}
              </li>
            ))}
          </ul>
        )}
      </Card>
      <Dialog
        open={confirmApply && preview !== null}
        onClose={() => setConfirmApply(false)}
        title={t("import.confirm.title")}
        size="sm"
      >
        {preview && (
          <div className="flex flex-col gap-3 text-sm" data-testid="import-confirm">
            <ul className="list-disc ps-5">
              <li>{t("import.dry.create", { count: preview.summary.create })}</li>
              <li>{t("import.dry.update", { count: preview.summary.update })}</li>
              <li>{t("import.dry.skip", { count: preview.summary.skip })}</li>
              <li>{t("import.dry.error", { count: preview.summary.error })}</li>
            </ul>
            <p className="text-muted">{t("import.confirm.undo")}</p>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setConfirmApply(false)}>
                {t("action.cancel")}
              </Button>
              <Button
                loading={busy}
                onClick={() => {
                  setConfirmApply(false);
                  void applyNow();
                }}
              >
                {t("import.apply.confirm")}
              </Button>
            </div>
          </div>
        )}
      </Dialog>
      <Dialog
        open={confirmRollback !== null}
        onClose={() => setConfirmRollback(null)}
        title={t("import.history.rollbackConfirmTitle")}
        size="sm"
      >
        <p className="text-sm text-text">
          {t("import.history.rollbackConfirmBody", {
            name: confirmRollback?.file_name ?? "",
            created: confirmRollback?.created_rows ?? 0,
            updated: confirmRollback?.updated_rows ?? 0,
          })}
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="secondary" onClick={() => setConfirmRollback(null)}>
            {t("action.cancel")}
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              const b = confirmRollback;
              if (!b) return;
              setConfirmRollback(null);
              void api
                .rollbackImport(b.id)
                .then(() => {
                  toast.push({ kind: "success", title: t("import.history.rollbackDone") });
                  void batches.reload();
                })
                .catch(fail);
            }}
          >
            {t("import.history.rollback")}
          </Button>
        </div>
      </Dialog>
    </>
  );
}

/**
 * The dry run in words, above the rows: nothing has been saved; what will be created,
 * updated, skipped and refused; why; what the school has to set up first; and which rows
 * look like a child who is already on file.
 */
function DryRunPanel({ preview, onDownload }: { preview: ImportPreview; onDownload: () => void }) {
  const { t } = useI18n();
  const { summary } = preview;
  // A check kept from before this panel existed (a draft restored after an update) has no
  // report: show the counts and nothing more.
  const report: DryRun = preview.report ?? {
    reasons: [],
    missing_classes: [],
    missing_sections: [],
    likely_duplicates: [],
  };
  const rowsOf = (rows: number[], count: number) =>
    rows.join(", ") + (count > rows.length ? " …" : "");
  const missing = [
    ...report.missing_classes.map((m) => ({ ...m, kind: "class" as const })),
    ...report.missing_sections.map((m) => ({ ...m, kind: "section" as const })),
  ];
  return (
    <section
      aria-label={t("import.dry.title")}
      data-testid="dry-run"
      className="border-b border-border px-4 py-3 text-sm"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 className="font-semibold text-navy">{t("import.dry.title")}</h3>
          <p className="text-xs text-muted">{t("import.dry.nothingSaved")}</p>
        </div>
        <Button
          size="sm"
          variant="secondary"
          icon={<Download className="h-3.5 w-3.5" aria-hidden />}
          onClick={onDownload}
        >
          {t("import.dry.download")}
        </Button>
      </div>
      <dl className="mt-2 grid grid-cols-2 gap-2 md:grid-cols-4">
        {(
          [
            ["create", summary.create, "text-success"],
            ["update", summary.update, "text-blue"],
            ["skip", summary.skip, "text-muted"],
            ["error", summary.error, summary.error > 0 ? "text-danger" : "text-muted"],
          ] as const
        ).map(([key, count, tone]) => (
          <div
            key={key}
            className="flex flex-col-reverse rounded border border-border bg-card px-3 py-2"
            data-testid={`dry-${key}`}
          >
            <dt className="text-xs text-muted">{t(`import.dry.label.${key}`)}</dt>
            <dd className={cn("text-xl font-semibold tabular-nums", tone)}>{count}</dd>
          </div>
        ))}
      </dl>
      {missing.length > 0 && (
        <div className="mt-3" data-testid="dry-missing">
          <h4 className="font-medium text-text">{t("import.dry.missing")}</h4>
          <p className="text-xs text-muted">{t("import.dry.missing.hint")}</p>
          <ul className="mt-1 list-disc ps-5 text-xs">
            {missing.map((m) => (
              <li key={`${m.kind}-${m.name}`}>
                <span className="font-medium">
                  {t(`import.dry.missing.${m.kind}`, { name: m.name })}
                </span>{" "}
                <span className="text-muted">
                  {t("import.dry.rows", { rows: rowsOf(m.rows, m.rows.length) })}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
      {report.likely_duplicates.length > 0 && (
        <div className="mt-3" data-testid="dry-duplicates">
          <h4 className="font-medium text-text">
            {t("import.dry.duplicates", { count: report.likely_duplicates.length })}
          </h4>
          <p className="text-xs text-muted">{t("import.dry.duplicates.hint")}</p>
          <ul className="mt-1 list-disc ps-5 text-xs">
            {report.likely_duplicates.slice(0, 20).map((d) => (
              <li key={d.row_no}>
                {t("import.dry.duplicates.item", {
                  row: d.row_no,
                  name: d.name,
                  like: d.looks_like,
                })}
              </li>
            ))}
          </ul>
        </div>
      )}
      {report.reasons.length > 0 && (
        <div className="mt-3" data-testid="dry-reasons">
          <h4 className="font-medium text-text">{t("import.dry.reasons")}</h4>
          <ul className="mt-1 text-xs">
            {report.reasons.map((r) => (
              <li
                key={`${r.action}-${r.reason}`}
                className="flex gap-2 border-t border-border py-1"
              >
                <span className="w-24 shrink-0">
                  <ActionBadge action={r.action} t={t} />
                </span>
                <span className="flex-1">
                  {r.reason}{" "}
                  <span className="text-muted">
                    {t("import.dry.rows", { rows: rowsOf(r.rows, r.count) })}
                  </span>
                </span>
                <span className="tabular-nums text-muted">{r.count}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

function ActionBadge({
  action,
  t,
}: {
  action: RowOutcome["action"];
  t: (k: TranslationKey) => string;
}) {
  const tone =
    action === "create"
      ? "success"
      : action === "update"
        ? "info"
        : action === "skip"
          ? "neutral"
          : "danger";
  return <Badge tone={tone}>{t(`import.action.${action}`)}</Badge>;
}

function MappingTable({
  fields,
  headers,
  sample,
  mapping,
  derived = {},
  onChange,
  t,
}: {
  fields: ImportField[];
  headers: string[];
  sample: string[];
  mapping: Record<string, number>;
  /** Required fields filled from other columns, with the hint key to show. */
  derived?: Record<string, "full_name" | "father_name">;
  onChange: (m: Record<string, number>) => void;
  t: (k: TranslationKey) => string;
}) {
  return (
    <table className="w-full text-sm">
      <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
        <tr>
          <th className="px-3 py-2 text-start">{t("import.mapping.field")}</th>
          <th className="px-3 py-2 text-start">{t("import.mapping.column")}</th>
          <th className="px-3 py-2 text-start">{t("import.preview.row")} 2</th>
        </tr>
      </thead>
      <tbody>
        {fields.map((f) => {
          const col = mapping[f.key];
          const from = derived[f.key];
          return (
            <tr key={f.key} className="border-t border-border">
              <td className="px-3 py-1.5">
                {f.label}{" "}
                {from ? (
                  <span className="text-xs text-success">
                    ({t(`import.mapping.derived.${from}`)})
                  </span>
                ) : (
                  f.required && (
                    <span className="text-xs text-danger">({t("import.mapping.required")})</span>
                  )
                )}
              </td>
              <td className="px-3 py-1.5">
                <select
                  aria-label={f.label}
                  value={col === undefined ? "" : String(col)}
                  onChange={(e) => {
                    const next = { ...mapping };
                    if (e.target.value === "") delete next[f.key];
                    else next[f.key] = Number(e.target.value);
                    onChange(next);
                  }}
                  className={cn(
                    "h-8 w-full rounded border bg-card px-2",
                    f.required && col === undefined && !derived[f.key]
                      ? "border-danger"
                      : "border-border",
                  )}
                >
                  <option value="">{t("import.mapping.ignore")}</option>
                  {headers.map((h, i) => (
                    <option key={i} value={i}>
                      {h || `(${i + 1})`}
                    </option>
                  ))}
                </select>
              </td>
              <td className="px-3 py-1.5 text-muted">
                {col === undefined ? "" : (sample[col] ?? "")}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
