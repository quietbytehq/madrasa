import { Fragment, useEffect, useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { AssessmentType, FormulaOp, GradingScheme, Term } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { num, useExamContext } from "../common";

export function SetupTab() {
  const { t } = useI18n();
  const { can } = useSession();
  const toast = useToast();
  const ctx = useExamContext();
  const canEdit = can("academics.manage");
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const saved = () => toast.push({ kind: "success", title: t("common.saved") });
  if (ctx.loading && !ctx.year)
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  if (!ctx.year) return <EmptyState title={t("exams.setup.noYear")} />;
  return (
    <div className="grid gap-4 xl:grid-cols-2">
      <StructureCard
        yearId={ctx.year.id}
        assessments={ctx.assessments}
        canEdit={canEdit}
        onChanged={() => void ctx.reloadAssessments()}
        onError={fail}
        onSaved={saved}
      />
      <GradingCard canEdit={canEdit} onError={fail} onSaved={saved} />
      <MaxMarksCard
        assessments={ctx.assessments.filter((a) => a.kind !== "calculated")}
        canEdit={canEdit}
        onError={fail}
        onSaved={saved}
      />
    </div>
  );
}

function StructureCard({
  yearId,
  assessments,
  canEdit,
  onChanged,
  onError,
  onSaved,
}: {
  yearId: string;
  assessments: AssessmentType[];
  canEdit: boolean;
  onChanged: () => void;
  onError: (e: unknown) => void;
  onSaved: () => void;
}) {
  const { t } = useI18n();
  const structure = useAsync(() => api.examStructure(yearId), [yearId, assessments]);
  const [weights, setWeights] = useState<Record<string, string>>({});
  const [termWeights, setTermWeights] = useState<Record<string, string>>({});
  const [name, setName] = useState("");
  const [termName, setTermName] = useState("");
  const [defaultTerms, setDefaultTerms] = useState("2");
  const [calc, setCalc] = useState<{
    termId: string;
    name: string;
    op: FormulaOp;
    inputs: Record<string, string>;
    n: string;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const terms = structure.data?.terms ?? [];
  const groups: Array<{ term: Term | null; components: AssessmentType[] }> = [
    ...terms.map((tm) => ({ term: tm as Term, components: tm.components })),
    ...(structure.data && structure.data.unassigned.length > 0
      ? [{ term: null, components: structure.data.unassigned }]
      : []),
  ];
  const weightOf = (a: AssessmentType) => weights[a.id] ?? num(a.weight);
  const termWeightOf = (tm: Term) => termWeights[tm.id] ?? num(tm.weight);
  const groupTotal = (components: AssessmentType[]) =>
    components.reduce((sum, a) => sum + (Number(weightOf(a)) || 0), 0);
  const termsTotal = terms.reduce((sum, tm) => sum + (Number(termWeightOf(tm)) || 0), 0);
  const allGroupsOk = groups.every((g) => Math.abs(groupTotal(g.components) - 100) < 0.001);
  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
      setWeights({});
      setTermWeights({});
      onChanged();
      await structure.reload();
      onSaved();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  const addEntered = (e: FormEvent, termId: string | null) => {
    e.preventDefault();
    if (!name.trim()) return;
    void run(() =>
      api
        .createAssessment({
          academic_year_id: yearId,
          name: name.trim(),
          position: assessments.length,
          term_id: termId ?? undefined,
        })
        .then(() => setName("")),
    );
  };
  const addTerm = (e: FormEvent) => {
    e.preventDefault();
    if (!termName.trim()) return;
    void run(() =>
      api
        .createTerm({ academic_year_id: yearId, name: termName.trim(), position: terms.length })
        .then(() => setTermName("")),
    );
  };
  const saveCalc = () => {
    if (!calc) return;
    const inputs = Object.entries(calc.inputs)
      .filter(([, w]) => w !== "")
      .map(([id, w]) => ({ assessment_type_id: id, weight: calc.op === "weighted" ? w : null }));
    void run(() =>
      api
        .createAssessment({
          academic_year_id: yearId,
          name: calc.name.trim(),
          position: assessments.length,
          term_id: calc.termId,
          kind: "calculated",
          weight: "0",
          formula: { op: calc.op, inputs, n: calc.op === "best_of" ? Number(calc.n) || 1 : null },
        })
        .then(() => setCalc(null)),
    );
  };
  const formulaText = (a: AssessmentType) => {
    if (!a.formula) return "";
    const names = a.formula.inputs
      .map((i) => {
        const n = assessments.find((x) => x.id === i.assessment_type_id)?.name ?? "?";
        return a.formula?.op === "weighted" && i.weight ? `${n} ×${num(i.weight)}` : n;
      })
      .join(", ");
    return `${t(`exams.setup.op.${a.formula.op}`)}${a.formula.op === "best_of" ? ` (${a.formula.n ?? 1})` : ""}: ${names}`;
  };
  if (structure.loading && !structure.data) return <Spinner />;
  return (
    <Card title={t("exams.setup.assessments")} padded={false}>
      <p className="px-4 pt-3 text-xs text-muted">{t("exams.setup.termWeightsHint")}</p>
      {assessments.length === 0 && terms.length === 0 ? (
        <div className="p-4">
          <EmptyState
            title={t("exams.setup.assessments")}
            action={
              canEdit && (
                <div className="flex flex-wrap items-end gap-2">
                  <Select
                    label={t("exams.setup.defaultsTerms")}
                    value={defaultTerms}
                    onChange={(e) => setDefaultTerms(e.target.value)}
                    options={[
                      { value: "2", label: t("exams.setup.defaultsTwo") },
                      { value: "1", label: t("exams.setup.defaultsOne") },
                    ]}
                    className="w-64"
                  />
                  <Button
                    loading={busy}
                    onClick={() =>
                      void run(() => api.defaultAssessments(yearId, Number(defaultTerms)))
                    }
                  >
                    {t("exams.setup.addDefaults")}
                  </Button>
                </div>
              )
            }
          />
        </div>
      ) : (
        <div className="flex flex-col">
          {terms.length > 0 && (
            <table className="w-full text-sm" aria-label={t("exams.setup.terms")}>
              <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-3 py-2 text-start">{t("exams.setup.termName")}</th>
                  <th className="w-32 px-3 py-2 text-start">{t("exams.setup.termWeight")}</th>
                  <th className="px-3 py-2 text-end" />
                </tr>
              </thead>
              <tbody>
                {terms.map((tm) => (
                  <tr key={tm.id} className="border-t border-border">
                    <td className="px-3 py-1.5">{tm.name}</td>
                    <td className="px-3 py-1.5">
                      <input
                        aria-label={`${tm.name}: ${t("exams.setup.termWeight")}`}
                        inputMode="decimal"
                        value={termWeightOf(tm)}
                        disabled={!canEdit}
                        onChange={(e) => setTermWeights((w) => ({ ...w, [tm.id]: e.target.value }))}
                        className="h-7 w-20 rounded border border-border bg-card px-2 text-end tabular-nums"
                      />
                    </td>
                    <td className="px-3 py-1.5 text-end">
                      {canEdit && tm.components.length === 0 && (
                        <Button
                          size="sm"
                          variant="ghost"
                          className="text-danger"
                          onClick={() => void run(() => api.deleteTerm(tm.id))}
                        >
                          {t("exams.setup.removeTerm")}
                        </Button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr className="border-t border-border text-xs">
                  <td className="px-3 py-2 text-muted">
                    {t("exams.setup.termWeightsTotal", { total: num(termsTotal) })}
                  </td>
                  <td colSpan={2} className="px-3 py-2 text-end">
                    {canEdit && (
                      <Button
                        size="sm"
                        loading={busy}
                        disabled={Math.abs(termsTotal - 100) > 0.001}
                        onClick={() =>
                          void run(() =>
                            api.setTermWeights(
                              yearId,
                              terms.map((tm) => ({ id: tm.id, weight: termWeightOf(tm) })),
                            ),
                          )
                        }
                      >
                        {t("exams.setup.saveTermWeights")}
                      </Button>
                    )}
                  </td>
                </tr>
              </tfoot>
            </table>
          )}
          {canEdit && (
            <form onSubmit={addTerm} className="flex items-end gap-2 border-t border-border p-3">
              <Input
                label={t("exams.setup.addTerm")}
                value={termName}
                onChange={(e) => setTermName(e.target.value)}
                className="w-56"
              />
              <Button type="submit" variant="secondary" disabled={!termName.trim()} loading={busy}>
                {t("exams.setup.addTerm")}
              </Button>
            </form>
          )}
          <table className="w-full text-sm" aria-label={t("exams.setup.components")}>
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2 text-start">{t("exams.setup.name")}</th>
                <th className="px-3 py-2 text-start">{t("exams.setup.kind")}</th>
                <th className="w-32 px-3 py-2 text-start">{t("exams.setup.weight")}</th>
                <th className="px-3 py-2 text-end" />
              </tr>
            </thead>
            <tbody>
              {groups.map((g) => (
                <Fragment key={g.term?.id ?? "none"}>
                  {groups.length > 1 && (
                    <tr className="border-t border-border bg-bg">
                      <th colSpan={4} className="px-3 py-1.5 text-start text-xs font-semibold">
                        {g.term
                          ? t("exams.setup.termTotal", {
                              term: g.term.name,
                              total: num(groupTotal(g.components)),
                            })
                          : "—"}
                      </th>
                    </tr>
                  )}
                  {g.components.map((a) => (
                    <tr key={a.id} className="border-t border-border">
                      <td className="px-3 py-1.5">
                        {a.name}
                        {a.kind === "calculated" && (
                          <div className="text-2xs text-muted">{formulaText(a)}</div>
                        )}
                      </td>
                      <td className="px-3 py-1.5 text-xs text-muted">
                        {t(`exams.setup.kind.${a.kind}`)}
                      </td>
                      <td className="px-3 py-1.5">
                        <input
                          aria-label={`${a.name}: ${t("exams.setup.weight")}`}
                          inputMode="decimal"
                          value={weightOf(a)}
                          disabled={!canEdit}
                          onChange={(e) => setWeights((w) => ({ ...w, [a.id]: e.target.value }))}
                          className="h-7 w-20 rounded border border-border bg-card px-2 text-end tabular-nums"
                        />
                      </td>
                      <td className="px-3 py-1.5 text-end">
                        {canEdit && terms.length > 1 && (
                          <select
                            aria-label={`${a.name}: ${t("exams.setup.moveTo")}`}
                            value={a.term_id ?? ""}
                            onChange={(e) =>
                              void run(() =>
                                api.renameAssessment(a.id, {
                                  name: a.name,
                                  position: a.position,
                                  term_id: e.target.value,
                                }),
                              )
                            }
                            className="me-2 h-7 rounded border border-border bg-card px-1 text-xs"
                          >
                            {terms.map((tm) => (
                              <option key={tm.id} value={tm.id}>
                                {tm.name}
                              </option>
                            ))}
                          </select>
                        )}
                        {canEdit && (
                          <Button
                            size="sm"
                            variant="ghost"
                            className="text-danger"
                            onClick={() => void run(() => api.deleteAssessment(a.id))}
                          >
                            {t("exams.setup.remove")}
                          </Button>
                        )}
                      </td>
                    </tr>
                  ))}
                </Fragment>
              ))}
            </tbody>
            <tfoot>
              <tr className="border-t border-border text-xs">
                <td colSpan={2} className="px-3 py-2 text-muted">
                  {groups.length === 1
                    ? t("exams.setup.weightsTotal", {
                        total: num(groupTotal(groups[0]?.components ?? [])),
                      })
                    : t("exams.setup.weightsHint")}
                </td>
                <td colSpan={2} className="px-3 py-2 text-end">
                  {canEdit && (
                    <Button
                      size="sm"
                      loading={busy}
                      disabled={!allGroupsOk}
                      onClick={() =>
                        void run(() =>
                          api.setWeights(
                            yearId,
                            assessments.map((a) => ({ id: a.id, weight: weightOf(a) })),
                          ),
                        )
                      }
                    >
                      {t("exams.setup.saveWeights")}
                    </Button>
                  )}
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      )}
      {canEdit && (assessments.length > 0 || terms.length > 0) && (
        <div className="flex flex-col gap-2 border-t border-border p-3">
          <form
            onSubmit={(e) => addEntered(e, terms[0]?.id ?? null)}
            className="flex flex-wrap items-end gap-2"
          >
            <Input
              label={t("exams.setup.add")}
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-56"
            />
            <Button type="submit" variant="secondary" disabled={!name.trim()} loading={busy}>
              {t("exams.setup.add")}
            </Button>
            {terms.length > 0 && !calc && (
              <Button
                type="button"
                variant="secondary"
                onClick={() =>
                  setCalc({ termId: terms[0]!.id, name: "", op: "average", inputs: {}, n: "1" })
                }
              >
                {t("exams.setup.addCalculated")}
              </Button>
            )}
          </form>
          {calc && (
            <div
              className="flex flex-col gap-2 rounded border border-border p-3"
              data-testid="formula-editor"
            >
              <div className="flex flex-wrap items-end gap-2">
                <Input
                  label={t("exams.setup.name")}
                  value={calc.name}
                  onChange={(e) => setCalc({ ...calc, name: e.target.value })}
                  className="w-48"
                />
                <Select
                  label={t("exams.setup.termName")}
                  value={calc.termId}
                  onChange={(e) => setCalc({ ...calc, termId: e.target.value, inputs: {} })}
                  options={terms.map((tm) => ({ value: tm.id, label: tm.name }))}
                  className="w-40"
                />
                <Select
                  label={t("exams.setup.formula")}
                  value={calc.op}
                  onChange={(e) => setCalc({ ...calc, op: e.target.value as FormulaOp })}
                  options={(["sum", "average", "weighted", "best_of"] as const).map((op) => ({
                    value: op,
                    label: t(`exams.setup.op.${op}`),
                  }))}
                  className="w-52"
                />
                {calc.op === "best_of" && (
                  <Input
                    label={t("exams.setup.bestN")}
                    type="number"
                    min={1}
                    value={calc.n}
                    onChange={(e) => setCalc({ ...calc, n: e.target.value })}
                    className="w-20"
                  />
                )}
              </div>
              <fieldset>
                <legend className="mb-1 text-xs font-medium text-text">
                  {t("exams.setup.inputs")}
                </legend>
                <div className="flex flex-wrap gap-3">
                  {(terms.find((tm) => tm.id === calc.termId)?.components ?? []).map((a) => (
                    <label key={a.id} className="flex items-center gap-1 text-sm">
                      <input
                        type="checkbox"
                        checked={a.id in calc.inputs}
                        onChange={(e) => {
                          const next = { ...calc.inputs };
                          if (e.target.checked) next[a.id] = "1";
                          else delete next[a.id];
                          setCalc({ ...calc, inputs: next });
                        }}
                      />
                      {a.name}
                      {calc.op === "weighted" && a.id in calc.inputs && (
                        <input
                          aria-label={`${a.name}: ${t("exams.setup.inputWeight")}`}
                          inputMode="decimal"
                          value={calc.inputs[a.id]}
                          onChange={(e) =>
                            setCalc({ ...calc, inputs: { ...calc.inputs, [a.id]: e.target.value } })
                          }
                          className="h-6 w-14 rounded border border-border bg-card px-1 text-end"
                        />
                      )}
                    </label>
                  ))}
                </div>
                {Object.keys(calc.inputs).length === 0 && (
                  <p className="text-xs text-muted">{t("exams.setup.noInputs")}</p>
                )}
              </fieldset>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  loading={busy}
                  disabled={!calc.name.trim() || Object.keys(calc.inputs).length === 0}
                  onClick={saveCalc}
                >
                  {t("action.save")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setCalc(null)}>
                  {t("action.cancel")}
                </Button>
              </div>
            </div>
          )}
        </div>
      )}
    </Card>
  );
}

function GradingCard({
  canEdit,
  onError,
  onSaved,
}: {
  canEdit: boolean;
  onError: (e: unknown) => void;
  onSaved: () => void;
}) {
  const { t } = useI18n();
  const grading = useAsync(() => api.grading(), []);
  const [edit, setEdit] = useState<GradingScheme | null>(null);
  const [busy, setBusy] = useState(false);
  const g = edit ?? grading.data;
  if (!g)
    return (
      <Card title={t("exams.setup.grading")}>
        <Spinner />
      </Card>
    );
  const set = (next: GradingScheme) => setEdit(next);
  const save = async () => {
    setBusy(true);
    try {
      grading.setData(await api.setGrading(g));
      setEdit(null);
      onSaved();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title={t("exams.setup.grading")} padded={false}>
      <p className="px-4 pt-3 text-xs text-muted">{t("exams.setup.gradingHint")}</p>
      <table className="w-full text-sm">
        <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
          <tr>
            <th className="px-3 py-2 text-start">{t("exams.setup.grade")}</th>
            <th className="px-3 py-2 text-start">{t("exams.setup.minPercent")}</th>
            <th className="px-3 py-2 text-start">{t("exams.setup.remarks")}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {g.bands.map((b, i) => (
            <tr key={i} className="border-t border-border">
              <td className="px-3 py-1">
                <input
                  aria-label={`${t("exams.setup.grade")} ${i + 1}`}
                  value={b.grade}
                  disabled={!canEdit}
                  onChange={(e) =>
                    set({
                      ...g,
                      bands: g.bands.map((x, j) => (j === i ? { ...x, grade: e.target.value } : x)),
                    })
                  }
                  className="h-7 w-20 rounded border border-border bg-card px-2"
                />
              </td>
              <td className="px-3 py-1">
                <input
                  aria-label={`${t("exams.setup.minPercent")} ${i + 1}`}
                  inputMode="decimal"
                  value={b.min_percent}
                  disabled={!canEdit}
                  onChange={(e) =>
                    set({
                      ...g,
                      bands: g.bands.map((x, j) =>
                        j === i ? { ...x, min_percent: e.target.value } : x,
                      ),
                    })
                  }
                  className="h-7 w-20 rounded border border-border bg-card px-2 text-end tabular-nums"
                />
              </td>
              <td className="px-3 py-1">
                <input
                  aria-label={`${t("exams.setup.remarks")} ${i + 1}`}
                  value={b.remarks}
                  disabled={!canEdit}
                  onChange={(e) =>
                    set({
                      ...g,
                      bands: g.bands.map((x, j) =>
                        j === i ? { ...x, remarks: e.target.value } : x,
                      ),
                    })
                  }
                  className="h-7 w-full rounded border border-border bg-card px-2"
                />
              </td>
              <td className="px-3 py-1 text-end">
                {canEdit && (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-danger"
                    onClick={() => set({ ...g, bands: g.bands.filter((_, j) => j !== i) })}
                  >
                    {t("exams.setup.remove")}
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {canEdit && (
        <div className="flex flex-wrap items-end gap-2 border-t border-border p-3">
          <Input
            label={t("exams.setup.passPercent")}
            inputMode="decimal"
            value={g.pass_percent}
            onChange={(e) => set({ ...g, pass_percent: e.target.value })}
            className="w-28"
          />
          <Button
            variant="secondary"
            onClick={() =>
              set({ ...g, bands: [...g.bands, { grade: "", min_percent: "0", remarks: "" }] })
            }
          >
            {t("exams.setup.addBand")}
          </Button>
          <span className="flex-1" />
          <Button loading={busy} disabled={!edit} onClick={() => void save()}>
            {t("exams.setup.saveGrading")}
          </Button>
        </div>
      )}
    </Card>
  );
}

function MaxMarksCard({
  assessments,
  canEdit,
  onError,
  onSaved,
}: {
  assessments: AssessmentType[];
  canEdit: boolean;
  onError: (e: unknown) => void;
  onSaved: () => void;
}) {
  const { t } = useI18n();
  const classes = useAsync(() => api.classes(), []);
  const [assessmentId, setAssessmentId] = useState("");
  const [classId, setClassId] = useState("");
  const [edits, setEdits] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!assessmentId && assessments.length > 0) setAssessmentId(assessments[0]!.id);
  }, [assessments, assessmentId]);
  useEffect(() => {
    if (!classId && classes.data && classes.data.length > 0) setClassId(classes.data[0]!.id);
  }, [classes.data, classId]);
  const lines = useAsync(
    () => (assessmentId && classId ? api.maxMarks(assessmentId, classId) : Promise.resolve(null)),
    [assessmentId, classId],
  );
  const valueOf = (subjectId: string, current: string | null) =>
    edits[subjectId] ?? (current ? num(current) : "");
  const save = async () => {
    setBusy(true);
    try {
      lines.setData(
        await api.setMaxMarks({
          assessment_id: assessmentId,
          class_id: classId,
          lines: Object.entries(edits).map(([subject_id, max]) => ({
            subject_id,
            max_marks: max.trim() === "" ? "0" : max.trim(),
          })),
        }),
      );
      setEdits({});
      onSaved();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title={t("exams.setup.maxMarks")} padded={false} className="xl:col-span-2">
      <div className="flex flex-wrap items-end gap-2 border-b border-border p-3">
        <Select
          label={t("exams.assessment")}
          value={assessmentId}
          onChange={(e) => {
            setAssessmentId(e.target.value);
            setEdits({});
          }}
          options={assessments.map((a) => ({ value: a.id, label: a.name }))}
          className="w-52"
        />
        <Select
          label={t("common.class")}
          value={classId}
          onChange={(e) => {
            setClassId(e.target.value);
            setEdits({});
          }}
          options={(classes.data ?? []).map((c) => ({ value: c.id, label: c.name }))}
          className="w-36"
        />
        <span className="text-xs text-muted">{t("exams.setup.maxHint")}</span>
        <span className="flex-1" />
        {canEdit && (
          <Button
            loading={busy}
            disabled={Object.keys(edits).length === 0}
            onClick={() => void save()}
          >
            {t("exams.setup.saveMax")}
          </Button>
        )}
      </div>
      {!lines.data ? (
        <div className="flex justify-center py-6">
          <Spinner />
        </div>
      ) : lines.data.length === 0 ? (
        <EmptyState title={t("exams.sheets.empty")} />
      ) : (
        <div className="grid gap-x-6 gap-y-2 p-3 sm:grid-cols-2 lg:grid-cols-3">
          {lines.data.map((l) => (
            <label key={l.subject_id} className="flex items-center justify-between gap-2 text-sm">
              <span>{l.subject_name}</span>
              <input
                aria-label={`${l.subject_name}: ${t("exams.setup.maxMarks")}`}
                inputMode="decimal"
                value={valueOf(l.subject_id, l.max_marks)}
                disabled={!canEdit}
                onChange={(e) => setEdits((all) => ({ ...all, [l.subject_id]: e.target.value }))}
                className="h-7 w-24 rounded border border-border bg-card px-2 text-end tabular-nums"
              />
            </label>
          ))}
        </div>
      )}
    </Card>
  );
}
