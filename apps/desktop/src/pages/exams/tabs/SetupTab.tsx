import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { AssessmentType, GradingScheme } from "@/api/types";
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
      <AssessmentsCard
        yearId={ctx.year.id}
        assessments={ctx.assessments}
        canEdit={canEdit}
        onChanged={() => void ctx.reloadAssessments()}
        onError={fail}
        onSaved={saved}
      />
      <GradingCard canEdit={canEdit} onError={fail} onSaved={saved} />
      <MaxMarksCard
        assessments={ctx.assessments}
        canEdit={canEdit}
        onError={fail}
        onSaved={saved}
      />
    </div>
  );
}

function AssessmentsCard({
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
  const [weights, setWeights] = useState<Record<string, string>>({});
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const weightOf = (a: AssessmentType) => weights[a.id] ?? num(a.weight);
  const total = assessments.reduce((sum, a) => sum + (Number(weightOf(a)) || 0), 0);
  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
      setWeights({});
      onChanged();
      onSaved();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  const add = (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    void run(() =>
      api
        .createAssessment({
          academic_year_id: yearId,
          name: name.trim(),
          position: assessments.length,
        })
        .then(() => setName("")),
    );
  };
  return (
    <Card title={t("exams.setup.assessments")} padded={false}>
      <p className="px-4 pt-3 text-xs text-muted">{t("exams.setup.weightsHint")}</p>
      {assessments.length === 0 ? (
        <div className="p-4">
          <EmptyState
            title={t("exams.setup.assessments")}
            action={
              canEdit && (
                <Button
                  loading={busy}
                  onClick={() => void run(() => api.defaultAssessments(yearId))}
                >
                  {t("exams.setup.addDefaults")}
                </Button>
              )
            }
          />
        </div>
      ) : (
        <table className="w-full text-sm">
          <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-3 py-2 text-start">{t("exams.setup.name")}</th>
              <th className="w-32 px-3 py-2 text-start">{t("exams.setup.weight")}</th>
              <th className="px-3 py-2 text-end" />
            </tr>
          </thead>
          <tbody>
            {assessments.map((a) => (
              <tr key={a.id} className="border-t border-border">
                <td className="px-3 py-1.5">{a.name}</td>
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
          </tbody>
          <tfoot>
            <tr className="border-t border-border text-xs">
              <td className="px-3 py-2 text-muted">
                {t("exams.setup.weightsTotal", { total: num(total) })}
              </td>
              <td colSpan={2} className="px-3 py-2 text-end">
                {canEdit && (
                  <Button
                    size="sm"
                    loading={busy}
                    disabled={Math.abs(total - 100) > 0.001}
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
      )}
      {canEdit && (
        <form onSubmit={add} className="flex items-end gap-2 border-t border-border p-3">
          <Input
            label={t("exams.setup.add")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="w-56"
          />
          <Button type="submit" variant="secondary" disabled={!name.trim()} loading={busy}>
            {t("exams.setup.add")}
          </Button>
        </form>
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
