import { useCallback, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { api } from "@/api/endpoints";
import type { MarksSheetView } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { AssessmentSectionPickers, num, SheetStatusBadge, useExamContext } from "../common";

export function MarksTab() {
  const { t } = useI18n();
  const ctx = useExamContext();
  const [sel, setSel] = useState({ assessmentId: "", sectionId: "" });
  const [subjectId, setSubjectId] = useState<string | null>(null);
  const onChange = useCallback((next: { assessmentId: string; sectionId: string }) => {
    setSel(next);
    setSubjectId(null);
  }, []);
  const sheets = useAsync(
    () =>
      sel.assessmentId && sel.sectionId
        ? api.examSheets(sel.assessmentId, sel.sectionId)
        : Promise.resolve(null),
    [sel.assessmentId, sel.sectionId],
  );
  if (ctx.loading && ctx.assessments.length === 0) {
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  }
  if (!ctx.year) return <EmptyState title={t("exams.setup.noYear")} />;
  if (ctx.assessments.length === 0)
    return (
      <EmptyState title={t("exams.setup.assessments")} description={t("exams.setup.addDefaults")} />
    );
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <AssessmentSectionPickers
          assessments={ctx.assessments}
          sections={ctx.sections}
          assessmentId={sel.assessmentId}
          sectionId={sel.sectionId}
          onChange={onChange}
        />
      </div>
      {subjectId ? (
        <MarksSheet
          key={`${sel.assessmentId}-${sel.sectionId}-${subjectId}`}
          assessmentId={sel.assessmentId}
          sectionId={sel.sectionId}
          subjectId={subjectId}
          onBack={() => {
            setSubjectId(null);
            void sheets.reload();
          }}
        />
      ) : (
        <Card padded={false}>
          {sheets.loading && !sheets.data ? (
            <div className="flex justify-center py-10">
              <Spinner />
            </div>
          ) : sheets.error ? (
            <EmptyState title={errorMessage(sheets.error, t("common.error.load"))} />
          ) : !sheets.data || sheets.data.sheets.length === 0 ? (
            <EmptyState title={t("exams.sheets.empty")} />
          ) : (
            <table className="w-full text-sm">
              <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-3 py-2 text-start">{t("exams.subject")}</th>
                  <th className="px-3 py-2 text-start">{t("exams.marks.max", { max: "" })}</th>
                  <th className="px-3 py-2 text-start">{t("common.status")}</th>
                  <th className="px-3 py-2 text-start">{t("exams.marks.obtained")}</th>
                  <th className="px-3 py-2 text-end">{t("common.actions")}</th>
                </tr>
              </thead>
              <tbody>
                {sheets.data.sheets.map((s) => (
                  <tr key={s.subject_id} className="border-t border-border">
                    <td className="px-3 py-1.5 font-medium">{s.subject_name}</td>
                    <td className="px-3 py-1.5 tabular-nums">
                      {s.max_marks ? (
                        num(s.max_marks)
                      ) : (
                        <span className="text-xs text-warning">{t("exams.sheets.noMax")}</span>
                      )}
                    </td>
                    <td className="px-3 py-1.5">
                      <SheetStatusBadge status={s.status} t={t} />
                    </td>
                    <td className="px-3 py-1.5 text-muted">
                      {t("exams.sheets.entered", { entered: s.entered, students: s.students })}
                    </td>
                    <td className="px-3 py-1.5 text-end">
                      {s.max_marks && (
                        <Button
                          size="sm"
                          variant={s.can_enter ? "primary" : "secondary"}
                          onClick={() => setSubjectId(s.subject_id)}
                        >
                          {t("exams.sheets.open")}
                        </Button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      )}
    </div>
  );
}

type Entry = { obtained: string; absent: boolean };

function MarksSheet({
  assessmentId,
  sectionId,
  subjectId,
  onBack,
}: {
  assessmentId: string;
  sectionId: string;
  subjectId: string;
  onBack: () => void;
}) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const sheet = useAsync(
    () => api.examSheet(assessmentId, sectionId, subjectId),
    [assessmentId, sectionId, subjectId],
  );
  const [edits, setEdits] = useState<Record<string, Entry>>({});
  const [busy, setBusy] = useState(false);
  const inputs = useRef<Array<HTMLInputElement | null>>([]);
  const v: MarksSheetView | null = sheet.data;
  const max = v?.max_marks ? Number(v.max_marks) : null;
  const entryOf = useMemo(
    () =>
      (s: MarksSheetView["students"][number]): Entry =>
        edits[s.student_id] ?? { obtained: s.obtained ? num(s.obtained) : "", absent: s.is_absent },
    [edits],
  );
  const invalid = (e: Entry) => {
    if (e.absent || e.obtained.trim() === "") return false;
    const n = Number(e.obtained);
    return !/^\d+(\.\d{1,2})?$/.test(e.obtained.trim()) || n < 0 || (max !== null && n > max);
  };
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const run = async (f: () => Promise<MarksSheetView>, success?: string) => {
    setBusy(true);
    try {
      sheet.setData(await f());
      setEdits({});
      if (success) toast.push({ kind: "success", title: success });
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const save = () =>
    run(
      () =>
        api.saveMarks({
          assessment_id: assessmentId,
          section_id: sectionId,
          subject_id: subjectId,
          marks: (v?.students ?? []).map((s) => {
            const e = entryOf(s);
            return {
              student_id: s.student_id,
              obtained: e.absent || e.obtained.trim() === "" ? null : e.obtained.trim(),
              is_absent: e.absent,
            };
          }),
        }),
      t("exams.marks.saved"),
    );
  const onKey = (e: KeyboardEvent<HTMLInputElement>, i: number) => {
    if (e.key === "Enter" || e.key === "ArrowDown") {
      e.preventDefault();
      inputs.current[i + 1]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      inputs.current[i - 1]?.focus();
    }
  };

  if (sheet.loading && !v)
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  if (sheet.error || !v)
    return <EmptyState title={errorMessage(sheet.error, t("common.error.load"))} />;
  const anyInvalid = v.students.some((s) => invalid(entryOf(s)));
  const dirty = Object.keys(edits).length > 0;
  return (
    <Card
      padded={false}
      title={`${v.subject_name} · ${v.assessment.name} · ${v.section.class_name}-${v.section.section_name}`}
      actions={
        <div className="flex flex-wrap items-center gap-2">
          <SheetStatusBadge status={v.sheet ? v.status : null} t={t} />
          <Button size="sm" variant="ghost" onClick={onBack}>
            {t("action.back")}
          </Button>
          {v.can_edit && (
            <Button
              size="sm"
              loading={busy}
              disabled={anyInvalid || !dirty}
              onClick={() => void save()}
            >
              {t("exams.marks.save")}
            </Button>
          )}
          {v.can_submit && !dirty && (
            <Button
              size="sm"
              variant="secondary"
              loading={busy}
              onClick={() => void run(() => api.submitSheet(v.sheet!.id))}
            >
              {t("exams.marks.submit")}
            </Button>
          )}
          {v.can_approve && (
            <Button
              size="sm"
              loading={busy}
              onClick={() => void run(() => api.approveSheet(v.sheet!.id))}
            >
              {t("exams.marks.approve")}
            </Button>
          )}
          {v.can_reopen && (
            <Button
              size="sm"
              variant="secondary"
              loading={busy}
              onClick={() => void run(() => api.reopenSheet(v.sheet!.id))}
            >
              {t("exams.marks.reopen")}
            </Button>
          )}
        </div>
      }
    >
      <div className="flex flex-col gap-1 border-b border-border px-4 py-2 text-xs text-muted">
        {max === null ? (
          <p className="text-warning">{t("exams.marks.noMax")}</p>
        ) : (
          <p>{t("exams.marks.hint", { max: num(v.max_marks) })}</p>
        )}
        {v.sheet?.submitted_at && (
          <p>
            {t("exams.marks.submittedBy", {
              name: v.submitted_by_name ?? "—",
              time: formatDate(v.sheet.submitted_at),
            })}{" "}
            {v.sheet.approved_at &&
              t("exams.marks.approvedBy", {
                name: v.approved_by_name ?? "—",
                time: formatDate(v.sheet.approved_at),
              })}
          </p>
        )}
        {v.sheet && v.status !== "draft" && !v.can_reopen && (
          <p className="text-text">{t("exams.marks.locked")}</p>
        )}
        {anyInvalid && (
          <p role="alert" className="text-danger">
            {t("exams.marks.invalid", { max: num(v.max_marks) })}
          </p>
        )}
      </div>
      <table className="w-full text-sm">
        <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
          <tr>
            <th className="w-14 px-3 py-2 text-start">{t("attendance.mark.rollNo")}</th>
            <th className="px-3 py-2 text-start">{t("students.fullName")}</th>
            <th className="w-40 px-3 py-2 text-start">
              {t("exams.marks.obtained")} / {num(v.max_marks)}
            </th>
            <th className="w-28 px-3 py-2 text-start">{t("exams.marks.absent")}</th>
          </tr>
        </thead>
        <tbody>
          {v.students.map((s, i) => {
            const e = entryOf(s);
            const bad = invalid(e);
            return (
              <tr
                key={s.student_id}
                className={cn("border-t border-border", bad && "bg-danger-bg/40")}
              >
                <td className="px-3 py-1 tabular-nums text-muted">{s.roll_no ?? "—"}</td>
                <td className="px-3 py-1">
                  <span className="font-medium">{s.full_name}</span>{" "}
                  <span className="text-xs text-muted">{s.student_number}</span>
                </td>
                <td className="px-3 py-1">
                  <input
                    ref={(el) => {
                      inputs.current[i] = el;
                    }}
                    aria-label={`${s.full_name}: ${t("exams.marks.obtained")}`}
                    aria-invalid={bad || undefined}
                    inputMode="decimal"
                    value={e.absent ? "" : e.obtained}
                    disabled={!v.can_edit || e.absent}
                    onChange={(ev) =>
                      setEdits((all) => ({
                        ...all,
                        [s.student_id]: { ...e, obtained: ev.target.value },
                      }))
                    }
                    onKeyDown={(ev) => onKey(ev, i)}
                    className={cn(
                      "h-7 w-24 rounded border bg-card px-2 text-end tabular-nums",
                      bad ? "border-danger" : "border-border",
                    )}
                  />
                </td>
                <td className="px-3 py-1">
                  <input
                    type="checkbox"
                    aria-label={`${s.full_name}: ${t("exams.marks.absent")}`}
                    checked={e.absent}
                    disabled={!v.can_edit}
                    onChange={(ev) =>
                      setEdits((all) => ({
                        ...all,
                        [s.student_id]: {
                          obtained: ev.target.checked ? "" : e.obtained,
                          absent: ev.target.checked,
                        },
                      }))
                    }
                  />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </Card>
  );
}
