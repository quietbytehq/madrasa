import { useEffect, useMemo, useState } from "react";
import { Download, Upload } from "lucide-react";
import { api } from "@/api/endpoints";
import type {
  AllocationImportPreview,
  AllocationParsedFile,
  SectionAllocations,
  Section,
} from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useSession } from "@/app/session";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";

type Draft = { group_id: string | null; subject_ids: string[] };

/** Bulk subject allocation per section (M2 stage 6) with an Excel import. */
export function SubjectAllocationPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const toast = useToast();
  const manage = can("students.manage");
  const years = useAsync(() => api.academicYears(), []);
  const classes = useAsync(() => api.classes(), []);
  const activeYear = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const sections = useAsync(
    () =>
      activeYear
        ? api.sections({ academic_year_id: activeYear.id })
        : Promise.resolve([] as Section[]),
    [activeYear?.id],
  );
  const [classId, setClassId] = useState("");
  const [sectionId, setSectionId] = useState("");
  const classSections = (sections.data ?? []).filter((s) => !classId || s.class_id === classId);
  const view = useAsync(
    () => (sectionId ? api.sectionAllocations(sectionId) : Promise.resolve(null)),
    [sectionId],
  );
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [busy, setBusy] = useState(false);
  const [showImport, setShowImport] = useState(false);
  useEffect(() => setDrafts({}), [view.data]);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });

  const data = view.data;
  const electives = data?.subjects.filter((s) => s.is_elective) ?? [];
  const draftOf = (st: SectionAllocations["students"][number]): Draft =>
    drafts[st.enrollment_id] ?? {
      group_id: st.group_id,
      subject_ids: st.allocated ? st.allocated_ids : [],
    };
  const setDraft = (id: string, d: Draft) => setDrafts((prev) => ({ ...prev, [id]: d }));
  const changed = Object.keys(drafts).length;
  const save = async () => {
    if (!sectionId || changed === 0) return;
    setBusy(true);
    try {
      await api.setSectionAllocations(
        sectionId,
        Object.entries(drafts).map(([enrollment_id, d]) => ({
          enrollment_id,
          group_id: d.group_id,
          subject_ids: d.subject_ids,
        })),
      );
      toast.push({ kind: "success", title: t("subjects.alloc.saved") });
      await view.reload();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const applyGroupToAll = (groupId: string) => {
    if (!data) return;
    const g = data.groups.find((x) => x.id === groupId);
    for (const st of data.students) {
      setDraft(st.enrollment_id, { group_id: g?.id ?? null, subject_ids: g?.subject_ids ?? [] });
    }
  };

  return (
    <>
      <PageHeader
        title={t("subjects.alloc.title")}
        crumbs={[
          { label: t("nav.dashboard"), to: "/" },
          { label: t("students.title"), to: "/students" },
          { label: t("subjects.alloc.title") },
        ]}
        actions={
          <div className="flex flex-wrap items-center gap-2">
            {sectionId && (
              <Button
                variant="secondary"
                icon={<Download className="h-4 w-4" aria-hidden />}
                onClick={() => void api.downloadAllocationTemplate(sectionId).catch(fail)}
              >
                {t("subjects.alloc.template")}
              </Button>
            )}
            {manage && (
              <Button
                variant="secondary"
                icon={<Upload className="h-4 w-4" aria-hidden />}
                onClick={() => setShowImport((v) => !v)}
              >
                {t("subjects.alloc.import")}
              </Button>
            )}
            {manage && sectionId && (
              <Button loading={busy} disabled={changed === 0} onClick={() => void save()}>
                {t("subjects.alloc.save")}
              </Button>
            )}
          </div>
        }
      />
      <div className="flex flex-col gap-4">
        {showImport && manage && (
          <ImportPanel
            onImported={async () => {
              await view.reload();
            }}
          />
        )}
        <Card>
          <div className="flex flex-wrap items-end gap-3">
            <Select
              label={t("common.class")}
              value={classId}
              onChange={(e) => {
                setClassId(e.target.value);
                setSectionId("");
              }}
              options={[
                { value: "", label: t("subjects.alloc.anyClass") },
                ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
              ]}
              className="w-48"
            />
            <Select
              label={t("common.section")}
              value={sectionId}
              onChange={(e) => setSectionId(e.target.value)}
              options={[
                { value: "", label: t("subjects.alloc.pickSection") },
                ...classSections.map((s) => ({
                  value: s.id,
                  label: `${(classes.data ?? []).find((c) => c.id === s.class_id)?.name ?? ""} ${s.name}`,
                })),
              ]}
              className="w-56"
            />
            {manage && data && data.groups.length > 0 && (
              <Select
                label={t("subjects.alloc.applyGroup")}
                value=""
                onChange={(e) => e.target.value && applyGroupToAll(e.target.value)}
                options={[
                  { value: "", label: t("subjects.alloc.none") },
                  ...data.groups.map((g) => ({ value: g.id, label: g.name })),
                ]}
                className="w-56"
              />
            )}
          </div>
        </Card>
        {!sectionId ? (
          <Card>
            <EmptyState title={t("subjects.alloc.pickSection")} />
          </Card>
        ) : view.loading && !data ? (
          <div className="flex justify-center py-10">
            <Spinner size="lg" />
          </div>
        ) : !data ? (
          <Card>
            <EmptyState title={errorMessage(view.error, t("common.error.load"))} />
          </Card>
        ) : electives.length === 0 ? (
          <Card>
            <EmptyState title={t("subjects.alloc.noElectives")} />
          </Card>
        ) : (
          <Card padded={false}>
            <table className="w-full text-sm" aria-label={t("subjects.alloc.title")}>
              <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="w-12 px-3 py-2 text-start">{t("students.rollNo")}</th>
                  <th className="px-3 py-2 text-start">{t("subjects.alloc.student")}</th>
                  <th className="w-44 px-3 py-2 text-start">{t("subjects.alloc.group")}</th>
                  {electives.map((s) => (
                    <th key={s.subject_id} className="px-2 py-2 text-center">
                      {s.code}
                    </th>
                  ))}
                  <th className="w-28 px-3 py-2 text-start">{t("common.status")}</th>
                </tr>
              </thead>
              <tbody>
                {data.students.map((st) => {
                  const d = draftOf(st);
                  const allocated = d.group_id !== null || d.subject_ids.length > 0;
                  return (
                    <tr key={st.enrollment_id} className="border-t border-border">
                      <td className="px-3 py-1.5 tabular-nums">{st.roll_no ?? "—"}</td>
                      <td className="px-3 py-1.5">
                        <div className="font-medium">{st.full_name}</div>
                        <div className="text-xs text-muted">{st.student_number}</div>
                      </td>
                      <td className="px-3 py-1.5">
                        <select
                          aria-label={`${st.full_name}: ${t("subjects.alloc.group")}`}
                          className="h-8 w-full rounded border border-border bg-card px-2 text-sm"
                          disabled={!manage}
                          value={d.group_id ?? ""}
                          onChange={(e) => {
                            const g = data.groups.find((x) => x.id === e.target.value);
                            setDraft(st.enrollment_id, {
                              group_id: g?.id ?? null,
                              subject_ids: g ? g.subject_ids : d.subject_ids,
                            });
                          }}
                        >
                          <option value="">{t("subjects.alloc.none")}</option>
                          {data.groups.map((g) => (
                            <option key={g.id} value={g.id}>
                              {g.name}
                            </option>
                          ))}
                        </select>
                      </td>
                      {electives.map((s) => {
                        const takes = allocated
                          ? d.subject_ids.includes(s.subject_id)
                          : st.subject_ids.includes(s.subject_id);
                        return (
                          <td key={s.subject_id} className="px-2 py-1.5 text-center">
                            <input
                              type="checkbox"
                              className="h-4 w-4 accent-blue"
                              aria-label={`${st.full_name}: ${s.name}`}
                              disabled={!manage}
                              checked={takes}
                              onChange={(e) =>
                                setDraft(st.enrollment_id, {
                                  group_id: d.group_id,
                                  subject_ids: e.target.checked
                                    ? [...new Set([...d.subject_ids, s.subject_id])]
                                    : d.subject_ids.filter((x) => x !== s.subject_id),
                                })
                              }
                            />
                          </td>
                        );
                      })}
                      <td className="px-3 py-1.5">
                        <div className="flex items-center gap-1">
                          <Badge tone={allocated ? "success" : "neutral"}>
                            {allocated
                              ? t("subjects.alloc.allocated")
                              : t("subjects.alloc.allSubjects")}
                          </Badge>
                          {manage && allocated && (
                            <button
                              type="button"
                              className="text-xs text-muted underline"
                              onClick={() =>
                                setDraft(st.enrollment_id, { group_id: null, subject_ids: [] })
                              }
                            >
                              {t("subjects.alloc.clear")}
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </Card>
        )}
      </div>
    </>
  );
}

const FIELDS = ["student_number", "group", "subjects"] as const;

function ImportPanel({ onImported }: { onImported: () => Promise<void> }) {
  const { t } = useI18n();
  const toast = useToast();
  const [parsed, setParsed] = useState<AllocationParsedFile | null>(null);
  const [mapping, setMapping] = useState<Record<string, number>>({});
  const [preview, setPreview] = useState<AllocationImportPreview | null>(null);
  const [busy, setBusy] = useState<"parse" | "preview" | "apply" | "rollback" | null>(null);
  const [lastBatch, setLastBatch] = useState<string | null>(null);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const options = [
    { value: "", label: t("subjects.alloc.field.ignore") },
    ...FIELDS.map((f) => ({ value: f, label: t(`subjects.alloc.field.${f}`) })),
  ];
  const keyFor = (col: number) => Object.entries(mapping).find(([, i]) => i === col)?.[0] ?? "";
  const setKey = (col: number, key: string) =>
    setMapping((m) => {
      const next = Object.fromEntries(Object.entries(m).filter(([k, i]) => i !== col && k !== key));
      if (key) next[key] = col;
      return next;
    });
  const upload = async (file: File | null) => {
    if (!file) return;
    setBusy("parse");
    setPreview(null);
    try {
      const content_base64 = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result).split(",")[1] ?? "");
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
      });
      const p = await api.parseAllocations({ file_name: file.name, content_base64 });
      setParsed(p);
      setMapping(p.mapping);
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };
  const body = () => ({
    file_name: parsed?.file_name,
    headers: parsed?.headers ?? [],
    rows: parsed?.rows ?? [],
    mapping,
  });
  const check = async () => {
    setBusy("preview");
    try {
      setPreview(await api.previewAllocations(body()));
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };
  const apply = async () => {
    setBusy("apply");
    try {
      const res = await api.applyAllocations(body());
      setLastBatch(res.batch_id);
      toast.push({
        kind: "success",
        title: t("subjects.alloc.applied", { changed: res.summary.changed }),
      });
      setParsed(null);
      setPreview(null);
      await onImported();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };
  const rollback = async () => {
    if (!lastBatch) return;
    setBusy("rollback");
    try {
      await api.rollbackImport(lastBatch);
      setLastBatch(null);
      toast.push({ kind: "success", title: t("subjects.alloc.rolledBack") });
      await onImported();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };
  return (
    <Card title={t("subjects.alloc.import")}>
      <div className="flex flex-col gap-3" data-testid="allocation-import">
        <p className="text-xs text-muted">{t("subjects.alloc.importHint")}</p>
        <div className="flex flex-wrap items-center gap-3">
          <label className="inline-flex cursor-pointer items-center gap-2 rounded border border-border bg-card px-2.5 py-1.5 text-sm font-medium text-text hover:bg-hover">
            <Upload className="h-4 w-4" aria-hidden />
            {t("subjects.alloc.file")}
            <input
              type="file"
              accept=".xlsx,.xlsm,.xls,.csv"
              className="sr-only"
              aria-label={t("subjects.alloc.file")}
              onChange={(e) => {
                const f = e.target.files?.[0] ?? null;
                e.target.value = "";
                void upload(f);
              }}
            />
          </label>
          {busy === "parse" && <Spinner size="sm" />}
          {lastBatch && (
            <Button
              size="sm"
              variant="secondary"
              loading={busy === "rollback"}
              onClick={() => void rollback()}
            >
              {t("subjects.alloc.rollback")}
            </Button>
          )}
        </div>
        {parsed && (
          <>
            <div>
              <div className="mb-1 text-xs font-medium text-text">
                {t("subjects.alloc.mapping")}
              </div>
              <div className="flex flex-wrap gap-2">
                {parsed.headers.map((h, i) => (
                  <Select
                    key={i}
                    label={h || `#${i + 1}`}
                    value={keyFor(i)}
                    onChange={(e) => {
                      setKey(i, e.target.value);
                      setPreview(null);
                    }}
                    options={options}
                    className="w-48"
                  />
                ))}
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="secondary" loading={busy === "preview"} onClick={() => void check()}>
                {t("subjects.alloc.check")}
              </Button>
              <Button
                loading={busy === "apply"}
                disabled={!preview || preview.summary.errors > 0 || preview.summary.changed === 0}
                onClick={() => void apply()}
              >
                {t("subjects.alloc.apply")}
              </Button>
              {preview && (
                <span className="text-xs text-muted">
                  {t("subjects.alloc.summary", {
                    rows: preview.summary.rows,
                    changed: preview.summary.changed,
                    unchanged: preview.summary.unchanged,
                    errors: preview.summary.errors,
                  })}
                </span>
              )}
            </div>
            {preview && (
              <table className="w-full text-xs">
                <thead className="bg-hover uppercase tracking-wide text-muted">
                  <tr>
                    <th className="px-2 py-1 text-start">{t("subjects.alloc.row")}</th>
                    <th className="px-2 py-1 text-start">{t("subjects.alloc.student")}</th>
                    <th className="px-2 py-1 text-start">{t("subjects.alloc.group")}</th>
                    <th className="px-2 py-1 text-start">{t("subjects.alloc.electives")}</th>
                    <th className="px-2 py-1 text-start">{t("common.status")}</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((r) => (
                    <tr key={r.row_no} className="border-t border-border">
                      <td className="px-2 py-1 tabular-nums">{r.row_no}</td>
                      <td className="px-2 py-1">
                        {r.student_name ?? r.student_number}
                        {r.class_name && <span className="text-muted"> · {r.class_name}</span>}
                      </td>
                      <td className="px-2 py-1">{r.group_name ?? "—"}</td>
                      <td className="px-2 py-1">{r.subject_names.join(", ") || "—"}</td>
                      <td className="px-2 py-1">
                        {r.errors.length > 0 ? (
                          <span className="text-danger">{r.errors.join(" ")}</span>
                        ) : (
                          <>
                            <Badge tone={r.changed ? "info" : "neutral"}>
                              {r.changed
                                ? t("subjects.alloc.changes")
                                : t("subjects.alloc.unchanged")}
                            </Badge>
                            {r.warnings.length > 0 && (
                              <span className="ms-1 text-warning">{r.warnings.join(" ")}</span>
                            )}
                          </>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </>
        )}
      </div>
    </Card>
  );
}
