import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowDown, ArrowUp, Play, Plus, Save, X } from "lucide-react";
import { api } from "@/api/endpoints";
import type {
  BuilderCatalogue,
  BuilderField,
  BuilderOp,
  BuilderSource,
  ReportDefinition,
  ReportDocument,
  ReportFilter,
  ReportLang,
} from "@/api/types";
import { useSession } from "@/app/session";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";
import { ReportTable } from "./ReportTable";

const NO_VALUE: BuilderOp[] = ["is_empty", "not_empty", "is_true", "is_false"];

function emptyDefinition(source: string): ReportDefinition {
  return { source, columns: [], filters: [], sort: [], group_by: null, limit: null };
}

/** `/reports/custom/new` and `/reports/custom/:id/edit`: build a report from the catalogue. */
export function CustomReportBuilder() {
  const { t, locale } = useI18n();
  const lang: ReportLang = locale === "ur" ? "ur" : "en";
  const { id } = useParams();
  const navigate = useNavigate();
  const toast = useToast();
  const { can } = useSession();
  const catalogue = useAsync(() => api.builderCatalogue(lang), [lang]);
  const existing = useAsync(
    () =>
      id
        ? api.customReports().then((l) => l.find((r) => r.id === id) ?? null)
        : Promise.resolve(null),
    [id],
  );
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [shared, setShared] = useState(false);
  const [def, setDef] = useState<ReportDefinition | null>(null);
  const [preview, setPreview] = useState<ReportDocument | null>(null);
  const [busy, setBusy] = useState<"preview" | "save" | null>(null);

  // Seed from the saved report, or start on the first source.
  useEffect(() => {
    if (!catalogue.data || def) return;
    if (id) {
      if (existing.loading) return;
      if (existing.data) {
        setName(existing.data.name);
        setDescription(existing.data.description);
        setShared(existing.data.is_shared);
        setDef(existing.data.definition);
      }
      return;
    }
    const first = catalogue.data.sources[0];
    if (first) setDef(emptyDefinition(first.key));
  }, [catalogue.data, existing.data, existing.loading, id, def]);

  const source: BuilderSource | undefined = useMemo(
    () => catalogue.data?.sources.find((s) => s.key === def?.source),
    [catalogue.data, def?.source],
  );
  const fieldOf = (key: string): BuilderField | undefined =>
    source?.fields.find((f) => f.key === key);
  const update = (patch: Partial<ReportDefinition>) => setDef((d) => (d ? { ...d, ...patch } : d));

  if (!catalogue.data || !def || !source) {
    return (
      <div className="p-6">
        <Spinner />
      </div>
    );
  }
  const cat: BuilderCatalogue = catalogue.data;

  const changeSource = (key: string) => {
    setDef(emptyDefinition(key));
    setPreview(null);
  };
  const toggleColumn = (key: string) =>
    update({
      columns: def.columns.includes(key)
        ? def.columns.filter((c) => c !== key)
        : [...def.columns, key],
    });
  const move = (key: string, delta: number) => {
    const i = def.columns.indexOf(key);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= def.columns.length) return;
    const next = [...def.columns];
    [next[i], next[j]] = [next[j]!, next[i]!];
    update({ columns: next });
  };
  const setFilter = (i: number, patch: Partial<ReportFilter>) =>
    update({ filters: def.filters.map((f, k) => (k === i ? { ...f, ...patch } : f)) });
  const addFilter = () => {
    const first = source.fields[0];
    if (!first) return;
    update({
      filters: [
        ...def.filters,
        { field: first.key, op: first.operators[0] ?? "eq", value: "", value2: null },
      ],
    });
  };

  const valid = def.columns.length > 0 && name.trim().length > 0;
  const runPreview = async () => {
    setBusy("preview");
    try {
      setPreview(await api.previewCustomReport(def, name || t("reports.custom.title"), lang));
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  const save = async () => {
    setBusy("save");
    try {
      const body = {
        name: name.trim(),
        description: description.trim(),
        definition: def,
        is_shared: shared,
      };
      const saved = id
        ? await api.updateCustomReport(id, body)
        : await api.createCustomReport(body);
      toast.push({ kind: "success", title: t("reports.custom.saved") });
      navigate(`/reports/custom/${saved.id}`);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };

  const valueControl = (f: ReportFilter, i: number) => {
    const field = fieldOf(f.field);
    if (!field || NO_VALUE.includes(f.op)) return null;
    const label = t("reports.custom.value");
    if (field.type === "choice" && f.op !== "in") {
      return (
        <Select
          label={label}
          value={f.value}
          placeholder={label}
          onChange={(e) => setFilter(i, { value: e.target.value })}
          options={field.choices.map((c) => ({ value: c.value, label: c.label }))}
        />
      );
    }
    const type =
      field.type === "date"
        ? "date"
        : field.type === "number" || field.type === "money"
          ? "number"
          : "text";
    return (
      <>
        <Input
          label={f.op === "in" ? t("reports.custom.valueIn") : label}
          type={f.op === "in" ? "text" : type}
          value={f.value}
          onChange={(e) => setFilter(i, { value: e.target.value })}
        />
        {f.op === "between" && (
          <Input
            label={t("reports.custom.valueTo")}
            type={type}
            value={f.value2 ?? ""}
            onChange={(e) => setFilter(i, { value2: e.target.value })}
          />
        )}
      </>
    );
  };

  const opLabel = (op: BuilderOp) => cat.operators.find((o) => o.value === op)?.label ?? op;

  return (
    <>
      <PageHeader
        title={id ? t("reports.custom.edit") : t("reports.custom.new")}
        crumbs={[
          { label: t("nav.dashboard"), to: "/" },
          { label: t("reports.title"), to: "/reports" },
          { label: id ? t("reports.custom.edit") : t("reports.custom.new") },
        ]}
        actions={
          <Button variant="ghost" onClick={() => navigate("/reports")}>
            {t("action.cancel")}
          </Button>
        }
      />
      <div className="grid gap-4 lg:grid-cols-[22rem_1fr]">
        <div className="flex flex-col gap-4">
          <Card title={t("reports.custom.name")}>
            <div className="flex flex-col gap-3">
              <Input
                label={t("reports.custom.name")}
                value={name}
                required
                onChange={(e) => setName(e.target.value)}
              />
              <Input
                label={t("reports.custom.description")}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
              <Select
                label={t("reports.custom.source")}
                value={def.source}
                onChange={(e) => changeSource(e.target.value)}
                options={cat.sources.map((s) => ({ value: s.key, label: s.label }))}
              />
              {can("reports.manage") ? (
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={shared}
                    onChange={(e) => setShared(e.target.checked)}
                  />
                  {t("reports.custom.share")}
                </label>
              ) : (
                <p className="text-xs text-muted">{t("reports.custom.shareHint")}</p>
              )}
            </div>
          </Card>
          <Card title={t("reports.custom.columns")}>
            <ul className="flex flex-col gap-1 text-sm" aria-label={t("reports.custom.columns")}>
              {source.fields.map((f) => {
                const idx = def.columns.indexOf(f.key);
                return (
                  <li key={f.key} className="flex items-center gap-2">
                    <label className="flex flex-1 items-center gap-2">
                      <input
                        type="checkbox"
                        checked={idx >= 0}
                        onChange={() => toggleColumn(f.key)}
                      />
                      {f.label}
                    </label>
                    {idx >= 0 && (
                      <span className="flex items-center gap-1 text-xs text-muted">
                        {idx + 1}
                        <button
                          type="button"
                          aria-label={`${t("reports.custom.moveUp")}: ${f.label}`}
                          className="rounded p-0.5 hover:bg-hover disabled:opacity-40"
                          disabled={idx === 0}
                          onClick={() => move(f.key, -1)}
                        >
                          <ArrowUp className="h-3.5 w-3.5" aria-hidden />
                        </button>
                        <button
                          type="button"
                          aria-label={`${t("reports.custom.moveDown")}: ${f.label}`}
                          className="rounded p-0.5 hover:bg-hover disabled:opacity-40"
                          disabled={idx === def.columns.length - 1}
                          onClick={() => move(f.key, 1)}
                        >
                          <ArrowDown className="h-3.5 w-3.5" aria-hidden />
                        </button>
                      </span>
                    )}
                  </li>
                );
              })}
            </ul>
            {def.columns.length === 0 && (
              <p className="mt-2 text-xs text-danger">{t("reports.custom.noColumns")}</p>
            )}
          </Card>
        </div>
        <div className="flex flex-col gap-4">
          <Card
            title={t("reports.custom.filters")}
            actions={
              <Button
                size="sm"
                variant="secondary"
                icon={<Plus className="h-3.5 w-3.5" aria-hidden />}
                onClick={addFilter}
              >
                {t("reports.custom.addFilter")}
              </Button>
            }
          >
            <div className="flex flex-col gap-3">
              {def.filters.map((f, i) => {
                const field = fieldOf(f.field);
                return (
                  <div
                    key={i}
                    className="flex flex-wrap items-end gap-2 rounded border border-border p-2"
                  >
                    <Select
                      label={t("reports.custom.filters")}
                      value={f.field}
                      onChange={(e) => {
                        const nf = fieldOf(e.target.value);
                        setFilter(i, {
                          field: e.target.value,
                          op: nf?.operators[0] ?? "eq",
                          value: "",
                          value2: null,
                        });
                      }}
                      options={source.fields.map((x) => ({ value: x.key, label: x.label }))}
                      className="w-44"
                    />
                    <Select
                      label={t("reports.custom.operator")}
                      value={f.op}
                      onChange={(e) => setFilter(i, { op: e.target.value as BuilderOp })}
                      options={(field?.operators ?? []).map((op) => ({
                        value: op,
                        label: opLabel(op),
                      }))}
                      className="w-40"
                    />
                    <div className="flex items-end gap-2">{valueControl(f, i)}</div>
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={`${t("reports.custom.removeFilter")} ${i + 1}`}
                      icon={<X className="h-3.5 w-3.5" aria-hidden />}
                      onClick={() => update({ filters: def.filters.filter((_, k) => k !== i) })}
                    />
                  </div>
                );
              })}
              <div className="flex flex-wrap items-end gap-3">
                <Select
                  label={t("reports.custom.sort")}
                  value={def.sort[0]?.field ?? ""}
                  onChange={(e) =>
                    update({
                      sort: e.target.value
                        ? [{ field: e.target.value, desc: def.sort[0]?.desc ?? false }]
                        : [],
                    })
                  }
                  options={[
                    { value: "", label: t("reports.custom.sortNone") },
                    ...source.fields.map((x) => ({ value: x.key, label: x.label })),
                  ]}
                  className="w-44"
                />
                <label className="flex items-center gap-2 pb-2 text-sm">
                  <input
                    type="checkbox"
                    checked={def.sort[0]?.desc ?? false}
                    disabled={!def.sort[0]}
                    onChange={(e) =>
                      update({
                        sort: def.sort[0] ? [{ ...def.sort[0], desc: e.target.checked }] : [],
                      })
                    }
                  />
                  {t("reports.custom.desc")}
                </label>
                <Select
                  label={t("reports.custom.groupBy")}
                  value={def.group_by ?? ""}
                  onChange={(e) => update({ group_by: e.target.value || null })}
                  options={[
                    { value: "", label: t("reports.custom.groupNone") },
                    ...source.fields.map((x) => ({ value: x.key, label: x.label })),
                  ]}
                  className="w-44"
                />
                <Input
                  label={t("reports.custom.limit")}
                  type="number"
                  min={1}
                  max={cat.max_limit}
                  value={def.limit ?? cat.default_limit}
                  onChange={(e) => update({ limit: Number(e.target.value) || null })}
                  className="w-28"
                />
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="secondary"
                  loading={busy === "preview"}
                  disabled={def.columns.length === 0}
                  icon={<Play className="h-4 w-4" aria-hidden />}
                  onClick={() => void runPreview()}
                >
                  {t("reports.custom.preview")}
                </Button>
                <Button
                  loading={busy === "save"}
                  disabled={!valid}
                  icon={<Save className="h-4 w-4" aria-hidden />}
                  onClick={() => void save()}
                >
                  {t("reports.custom.save")}
                </Button>
                <span className="text-xs text-muted">{t("reports.custom.previewHint")}</span>
              </div>
            </div>
          </Card>
          {preview && (
            <Card>
              <div className="mb-2 text-xs text-muted">
                {t("reports.rows", { count: preview.row_count })}
              </div>
              <ReportTable report={preview} generatedLabel={t("reports.generated")} />
            </Card>
          )}
        </div>
      </div>
    </>
  );
}
