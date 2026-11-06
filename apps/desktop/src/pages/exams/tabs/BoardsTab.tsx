import { useEffect, useMemo, useState } from "react";
import { Plus, Trash2, Upload } from "lucide-react";
import { api } from "@/api/endpoints";
import type {
  BoardCandidate,
  BoardExamRequest,
  BoardExamView,
  BoardImportLayout,
  BoardImportPreview,
  BoardParsedFile,
  BoardRequest,
  BoardResultSheet,
  ExamBoard,
} from "@/api/types";
import { useSession } from "@/app/session";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { num, useExamContext } from "../common";

type BoardForm = {
  name: string;
  short_code: string;
  pass_percent: string;
  bands: Array<{ grade: string; min_percent: string }>;
};

const DEFAULT_BANDS = [
  { grade: "A+", min_percent: "80" },
  { grade: "A", min_percent: "70" },
  { grade: "B", min_percent: "60" },
  { grade: "C", min_percent: "50" },
  { grade: "D", min_percent: "40" },
  { grade: "E", min_percent: "33" },
];

function toForm(b: ExamBoard | null): BoardForm {
  return b
    ? {
        name: b.name,
        short_code: b.short_code,
        pass_percent: num(b.pass_percent),
        bands: b.grade_bands.map((x) => ({ grade: x.grade, min_percent: num(x.min_percent) })),
      }
    : { name: "", short_code: "", pass_percent: "33", bands: DEFAULT_BANDS };
}

/** Examinations → Boards: boards, their exams, candidates and imported results. */
export function BoardsTab() {
  const { t } = useI18n();
  const { can } = useSession();
  const toast = useToast();
  const manage = can("academics.manage");
  const { year } = useExamContext();
  const boards = useAsync(() => api.boards(), []);
  const [boardId, setBoardId] = useState<string | null>(null);
  const [editing, setEditing] = useState<ExamBoard | "new" | null>(null);
  const [examId, setExamId] = useState<string | null>(null);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const exams = useAsync(
    () =>
      boardId && year
        ? api.boardExams({ board_id: boardId, academic_year_id: year.id })
        : Promise.resolve([]),
    [boardId, year?.id],
  );
  useEffect(() => {
    if (!boardId && boards.data?.[0]) setBoardId(boards.data[0].id);
  }, [boards.data, boardId]);
  const board = boards.data?.find((b) => b.id === boardId) ?? null;
  const exam = exams.data?.find((e) => e.id === examId) ?? null;

  const removeBoard = async (b: ExamBoard) => {
    if (!window.confirm(t("boards.deleteConfirm", { name: b.name }))) return;
    try {
      await api.deleteBoard(b.id);
      toast.push({ kind: "success", title: t("boards.deleted") });
      setBoardId(null);
      await boards.reload();
    } catch (err) {
      fail(err);
    }
  };

  if (boards.loading && !boards.data) return <Spinner />;
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted">{t("boards.intro")}</p>
      <div className="grid gap-4 lg:grid-cols-[18rem_1fr]">
        <Card
          title={t("boards.title")}
          padded={false}
          actions={
            manage && (
              <Button
                size="sm"
                variant="secondary"
                icon={<Plus className="h-3.5 w-3.5" aria-hidden />}
                onClick={() => setEditing("new")}
              >
                {t("boards.add")}
              </Button>
            )
          }
        >
          {(boards.data ?? []).length === 0 ? (
            <p className="p-3 text-sm text-muted">{t("boards.none")}</p>
          ) : (
            <ul role="list" aria-label={t("boards.title")}>
              {(boards.data ?? []).map((b) => (
                <li key={b.id} className="flex items-center border-b border-border last:border-0">
                  <button
                    type="button"
                    aria-pressed={b.id === boardId}
                    onClick={() => {
                      setBoardId(b.id);
                      setExamId(null);
                    }}
                    className={`flex-1 px-3 py-2 text-start text-sm hover:bg-hover ${b.id === boardId ? "bg-selection font-medium text-blue" : "text-text"}`}
                  >
                    {b.name} <span className="text-2xs text-muted">{b.short_code}</span>
                  </button>
                  {manage && (
                    <>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => setEditing(b)}
                        aria-label={`${t("boards.edit")}: ${b.name}`}
                      >
                        {t("action.edit")}
                      </Button>
                      <button
                        type="button"
                        aria-label={`${t("action.delete")}: ${b.name}`}
                        className="p-1 text-muted hover:text-danger"
                        onClick={() => void removeBoard(b)}
                      >
                        <Trash2 className="h-3.5 w-3.5" aria-hidden />
                      </button>
                    </>
                  )}
                </li>
              ))}
            </ul>
          )}
        </Card>
        <div className="flex flex-col gap-4">
          {editing && (
            <BoardEditor
              board={editing === "new" ? null : editing}
              onDone={async (saved) => {
                setEditing(null);
                if (saved) {
                  await boards.reload();
                  setBoardId(saved.id);
                }
              }}
            />
          )}
          {board && !examId && (
            <ExamsPanel
              board={board}
              exams={exams.data ?? []}
              yearId={year?.id ?? null}
              manage={manage}
              onOpen={setExamId}
              onChanged={exams.reload}
            />
          )}
          {exam && (
            <ExamPanel
              key={exam.id}
              exam={exam}
              manage={manage}
              onBack={() => setExamId(null)}
              onChanged={exams.reload}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function BoardEditor({
  board,
  onDone,
}: {
  board: ExamBoard | null;
  onDone: (saved: ExamBoard | null) => Promise<void>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [form, setForm] = useState<BoardForm>(() => toForm(board));
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    try {
      const body: BoardRequest = {
        name: form.name,
        short_code: form.short_code,
        pass_percent: form.pass_percent,
        grade_bands: form.bands
          .filter((b) => b.grade.trim())
          .map((b) => ({ grade: b.grade.trim(), min_percent: b.min_percent })),
      };
      const saved = board ? await api.updateBoard(board.id, body) : await api.createBoard(body);
      toast.push({ kind: "success", title: t("boards.saved") });
      await onDone(saved);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const setBand = (i: number, patch: Partial<{ grade: string; min_percent: string }>) =>
    setForm((f) => ({ ...f, bands: f.bands.map((b, k) => (k === i ? { ...b, ...patch } : b)) }));
  return (
    <Card title={board ? t("boards.edit") : t("boards.add")}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <div className="flex flex-wrap gap-3">
          <Input
            label={t("boards.name")}
            value={form.name}
            required
            onChange={(e) => setForm({ ...form, name: e.target.value })}
            className="w-64"
          />
          <Input
            label={t("boards.code")}
            value={form.short_code}
            required
            onChange={(e) => setForm({ ...form, short_code: e.target.value })}
            className="w-28"
          />
          <Input
            label={t("boards.passPercent")}
            type="number"
            min={0}
            max={100}
            value={form.pass_percent}
            onChange={(e) => setForm({ ...form, pass_percent: e.target.value })}
            className="w-28"
          />
        </div>
        <div>
          <div className="mb-1 text-xs font-medium text-text">{t("boards.bands")}</div>
          <div className="flex flex-col gap-1">
            {form.bands.map((b, i) => (
              <div key={i} className="flex items-center gap-2">
                <Input
                  aria-label={`${t("boards.bandGrade")} ${i + 1}`}
                  value={b.grade}
                  onChange={(e) => setBand(i, { grade: e.target.value })}
                  className="w-20"
                />
                <Input
                  aria-label={`${t("boards.bandMin")} ${i + 1}`}
                  type="number"
                  min={0}
                  max={100}
                  value={b.min_percent}
                  onChange={(e) => setBand(i, { min_percent: e.target.value })}
                  className="w-24"
                />
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={`${t("action.delete")} ${t("boards.bandGrade")} ${i + 1}`}
                  icon={<Trash2 className="h-3.5 w-3.5" aria-hidden />}
                  onClick={() => setForm({ ...form, bands: form.bands.filter((_, k) => k !== i) })}
                />
              </div>
            ))}
            <div>
              <Button
                size="sm"
                variant="secondary"
                onClick={() =>
                  setForm({ ...form, bands: [...form.bands, { grade: "", min_percent: "" }] })
                }
              >
                {t("boards.addBand")}
              </Button>
            </div>
          </div>
        </div>
        <div className="flex gap-2">
          <Button type="submit" loading={busy}>
            {t("action.save")}
          </Button>
          <Button type="button" variant="ghost" onClick={() => void onDone(null)}>
            {t("action.cancel")}
          </Button>
        </div>
      </form>
    </Card>
  );
}

type SubjectRow = { name: string; subject_id: string; max_marks: string };

function ExamsPanel({
  board,
  exams,
  yearId,
  manage,
  onOpen,
  onChanged,
}: {
  board: ExamBoard;
  exams: BoardExamView[];
  yearId: string | null;
  manage: boolean;
  onOpen: (id: string) => void;
  onChanged: () => Promise<void>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const classes = useAsync(() => api.classes(), []);
  const subjects = useAsync(() => api.subjects(), []);
  const [editing, setEditing] = useState<BoardExamView | "new" | null>(null);
  const [name, setName] = useState("");
  const [status, setStatus] = useState<BoardExamView["status"]>("announced");
  const [classIds, setClassIds] = useState<string[]>([]);
  const [rows, setRows] = useState<SubjectRow[]>([]);
  const [busy, setBusy] = useState(false);
  const start = (e: BoardExamView | "new") => {
    setEditing(e);
    if (e === "new") {
      setName("");
      setStatus("announced");
      setClassIds([]);
      setRows([{ name: "", subject_id: "", max_marks: "100" }]);
    } else {
      setName(e.name);
      setStatus(e.status);
      setClassIds(e.class_ids);
      setRows(
        e.subjects.map((s) => ({
          name: s.name,
          subject_id: s.subject_id ?? "",
          max_marks: num(s.max_marks),
        })),
      );
    }
  };
  const save = async () => {
    setBusy(true);
    try {
      const body: BoardExamRequest = {
        board_id: board.id,
        academic_year_id: yearId ?? undefined,
        name,
        status,
        class_ids: classIds,
        subjects: rows
          .filter((r) => r.name.trim())
          .map((r) => ({
            name: r.name.trim(),
            subject_id: r.subject_id || null,
            max_marks: r.max_marks,
          })),
      };
      if (editing && editing !== "new") await api.updateBoardExam(editing.id, body);
      else await api.createBoardExam(body);
      toast.push({ kind: "success", title: t("boards.exam.saved") });
      setEditing(null);
      await onChanged();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const remove = async (e: BoardExamView) => {
    if (!window.confirm(t("boards.exam.deleteConfirm", { name: e.name }))) return;
    try {
      await api.deleteBoardExam(e.id);
      toast.push({ kind: "success", title: t("boards.exam.deleted") });
      await onChanged();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    }
  };
  return (
    <Card
      title={`${board.name} · ${t("boards.exams")}`}
      actions={
        manage && (
          <Button
            size="sm"
            variant="secondary"
            icon={<Plus className="h-3.5 w-3.5" aria-hidden />}
            onClick={() => start("new")}
          >
            {t("boards.exam.add")}
          </Button>
        )
      }
    >
      {editing && (
        <form
          className="mb-4 flex flex-col gap-3 rounded border border-border p-3"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <div className="flex flex-wrap gap-3">
            <Input
              label={t("boards.exam.name")}
              hint={t("boards.exam.nameHint")}
              value={name}
              required
              onChange={(e) => setName(e.target.value)}
              className="w-72"
            />
            {editing !== "new" && (
              <Select
                label={t("boards.exam.status")}
                value={status}
                onChange={(e) => setStatus(e.target.value as BoardExamView["status"])}
                options={(["announced", "results_imported", "closed"] as const).map((s) => ({
                  value: s,
                  label: t(`boards.exam.status.${s}`),
                }))}
                className="w-48"
              />
            )}
          </div>
          <fieldset>
            <legend className="mb-1 text-xs font-medium text-text">
              {t("boards.exam.classes")}
            </legend>
            <div className="flex flex-wrap gap-3">
              {(classes.data ?? []).map((c) => (
                <label key={c.id} className="flex items-center gap-1 text-sm">
                  <input
                    type="checkbox"
                    checked={classIds.includes(c.id)}
                    onChange={() =>
                      setClassIds(
                        classIds.includes(c.id)
                          ? classIds.filter((x) => x !== c.id)
                          : [...classIds, c.id],
                      )
                    }
                  />
                  {c.name}
                </label>
              ))}
            </div>
          </fieldset>
          <div>
            <div className="mb-1 text-xs font-medium text-text">{t("boards.exam.subjects")}</div>
            <div className="flex flex-col gap-1">
              {rows.map((r, i) => (
                <div key={i} className="flex flex-wrap items-center gap-2">
                  <Input
                    aria-label={`${t("boards.exam.subjectName")} ${i + 1}`}
                    value={r.name}
                    onChange={(e) =>
                      setRows(rows.map((x, k) => (k === i ? { ...x, name: e.target.value } : x)))
                    }
                    className="w-44"
                  />
                  <Select
                    aria-label={`${t("boards.exam.schoolSubject")} ${i + 1}`}
                    value={r.subject_id}
                    onChange={(e) =>
                      setRows(
                        rows.map((x, k) =>
                          k === i
                            ? {
                                ...x,
                                subject_id: e.target.value,
                                name:
                                  x.name ||
                                  (subjects.data?.find((s) => s.id === e.target.value)?.name ?? ""),
                              }
                            : x,
                        ),
                      )
                    }
                    options={[
                      { value: "", label: "—" },
                      ...(subjects.data ?? []).map((s) => ({ value: s.id, label: s.name })),
                    ]}
                    className="w-44"
                  />
                  <Input
                    aria-label={`${t("boards.exam.maxMarks")} ${i + 1}`}
                    type="number"
                    min={1}
                    value={r.max_marks}
                    onChange={(e) =>
                      setRows(
                        rows.map((x, k) => (k === i ? { ...x, max_marks: e.target.value } : x)),
                      )
                    }
                    className="w-24"
                  />
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`${t("action.delete")} ${t("boards.exam.subjectName")} ${i + 1}`}
                    icon={<Trash2 className="h-3.5 w-3.5" aria-hidden />}
                    onClick={() => setRows(rows.filter((_, k) => k !== i))}
                  />
                </div>
              ))}
              <div>
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => setRows([...rows, { name: "", subject_id: "", max_marks: "100" }])}
                >
                  {t("boards.exam.addSubject")}
                </Button>
              </div>
            </div>
          </div>
          <div className="flex gap-2">
            <Button type="submit" loading={busy}>
              {t("action.save")}
            </Button>
            <Button type="button" variant="ghost" onClick={() => setEditing(null)}>
              {t("action.cancel")}
            </Button>
          </div>
        </form>
      )}
      {exams.length === 0 && !editing ? (
        <p className="text-sm text-muted">{t("boards.exam.none")}</p>
      ) : (
        <ul className="divide-y divide-border">
          {exams.map((e) => (
            <li key={e.id} className="flex flex-wrap items-center gap-2 py-2">
              <button
                type="button"
                className="flex-1 text-start text-sm font-medium text-blue hover:underline"
                onClick={() => onOpen(e.id)}
              >
                {e.name}
              </button>
              <Badge
                tone={
                  e.status === "results_imported"
                    ? "success"
                    : e.status === "closed"
                      ? "neutral"
                      : "info"
                }
              >
                {t(`boards.exam.status.${e.status}`)}
              </Badge>
              <span className="text-xs text-muted">
                {e.class_names.join(", ")} ·{" "}
                {t("boards.exam.summary", { candidates: e.candidates, results: e.results })}
              </span>
              {manage && (
                <>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => start(e)}
                    aria-label={`${t("boards.exam.edit")}: ${e.name}`}
                  >
                    {t("action.edit")}
                  </Button>
                  <button
                    type="button"
                    aria-label={`${t("action.delete")}: ${e.name}`}
                    className="p-1 text-muted hover:text-danger"
                    onClick={() => void remove(e)}
                  >
                    <Trash2 className="h-3.5 w-3.5" aria-hidden />
                  </button>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

function ExamPanel({
  exam,
  manage,
  onBack,
  onChanged,
}: {
  exam: BoardExamView;
  manage: boolean;
  onBack: () => void;
  onChanged: () => Promise<void>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [view, setView] = useState<"candidates" | "results">("results");
  const candidates = useAsync(() => api.boardCandidates(exam.id), [exam.id]);
  const sheet = useAsync(() => api.boardResults(exam.id), [exam.id]);
  const classes = useAsync(() => api.classes(), []);
  const [regClass, setRegClass] = useState("");
  const [regGroup, setRegGroup] = useState("");
  const [busy, setBusy] = useState(false);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const refresh = async () => {
    await Promise.all([candidates.reload(), sheet.reload(), onChanged()]);
  };
  const register = async () => {
    if (!regClass) return;
    setBusy(true);
    try {
      const res = await api.registerCandidates(exam.id, {
        class_id: regClass,
        group_name: regGroup || undefined,
      });
      toast.push({ kind: "success", title: t("boards.candidates.added", { count: res.added }) });
      await refresh();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const saveCandidate = async (
    c: BoardCandidate,
    patch: { roll_no?: string; registration_no?: string },
  ) => {
    try {
      await api.updateCandidate(c.id, patch);
      await refresh();
    } catch (err) {
      fail(err);
    }
  };
  const removeCandidate = async (c: BoardCandidate) => {
    if (!window.confirm(t("boards.candidates.removeConfirm", { name: c.full_name }))) return;
    try {
      await api.removeCandidate(c.id);
      await refresh();
    } catch (err) {
      fail(err);
    }
  };
  return (
    <Card
      title={`${exam.board_name} · ${exam.name}`}
      actions={
        <div className="flex items-center gap-2">
          <Badge tone={exam.status === "results_imported" ? "success" : "info"}>
            {t(`boards.exam.status.${exam.status}`)}
          </Badge>
          <Button size="sm" variant="ghost" onClick={onBack}>
            {t("action.back")}
          </Button>
        </div>
      }
    >
      <div role="tablist" className="mb-3 flex gap-1 border-b border-border">
        {(["results", "candidates"] as const).map((v) => (
          <button
            key={v}
            role="tab"
            type="button"
            aria-selected={view === v}
            onClick={() => setView(v)}
            className={`px-3 py-1.5 text-sm ${view === v ? "border-b-2 border-blue font-medium text-blue" : "text-muted"}`}
          >
            {v === "results" ? t("boards.results") : t("boards.candidates")}
          </button>
        ))}
      </div>
      {view === "candidates" && (
        <div className="flex flex-col gap-3">
          {manage && (
            <div className="flex flex-wrap items-end gap-2 rounded border border-border p-2">
              <Select
                label={t("boards.candidates.wholeClass")}
                value={regClass}
                onChange={(e) => setRegClass(e.target.value)}
                options={[
                  { value: "", label: "—" },
                  ...(classes.data ?? [])
                    .filter((c) => exam.class_ids.includes(c.id))
                    .map((c) => ({ value: c.id, label: c.name })),
                ]}
                className="w-40"
              />
              <Input
                label={t("boards.candidates.group")}
                value={regGroup}
                onChange={(e) => setRegGroup(e.target.value)}
                className="w-32"
              />
              <Button loading={busy} disabled={!regClass} onClick={() => void register()}>
                {t("boards.candidates.register")}
              </Button>
              <span className="text-xs text-muted">{t("boards.candidates.registerHint")}</span>
            </div>
          )}
          {(candidates.data ?? []).length === 0 ? (
            <p className="text-sm text-muted">{t("boards.candidates.none")}</p>
          ) : (
            <table className="w-full text-sm">
              <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-2 py-1 text-start">{t("boards.col.student")}</th>
                  <th className="px-2 py-1 text-start">{t("boards.col.section")}</th>
                  <th className="px-2 py-1 text-start">{t("boards.candidates.rollNo")}</th>
                  <th className="px-2 py-1 text-start">{t("boards.candidates.regNo")}</th>
                  <th className="px-2 py-1 text-start">{t("boards.candidates.group")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {(candidates.data ?? []).map((c) => (
                  <tr key={c.id} className="border-t border-border">
                    <td className="px-2 py-1">
                      {c.full_name} <span className="text-xs text-muted">{c.student_number}</span>
                    </td>
                    <td className="px-2 py-1">
                      {c.class_name ? `${c.class_name}-${c.section_name ?? ""}` : "—"}
                    </td>
                    <td className="px-2 py-1">
                      <Input
                        aria-label={`${c.full_name}: ${t("boards.candidates.rollNo")}`}
                        defaultValue={c.roll_no ?? ""}
                        disabled={!manage}
                        onBlur={(e) =>
                          e.target.value !== (c.roll_no ?? "") &&
                          void saveCandidate(c, { roll_no: e.target.value })
                        }
                        className="w-32"
                      />
                    </td>
                    <td className="px-2 py-1">
                      <Input
                        aria-label={`${c.full_name}: ${t("boards.candidates.regNo")}`}
                        defaultValue={c.registration_no ?? ""}
                        disabled={!manage}
                        onBlur={(e) =>
                          e.target.value !== (c.registration_no ?? "") &&
                          void saveCandidate(c, { registration_no: e.target.value })
                        }
                        className="w-32"
                      />
                    </td>
                    <td className="px-2 py-1">{c.group_name ?? ""}</td>
                    <td className="px-2 py-1 text-end">
                      {manage && (
                        <button
                          type="button"
                          aria-label={`${t("boards.candidates.remove")}: ${c.full_name}`}
                          className="p-1 text-muted hover:text-danger"
                          onClick={() => void removeCandidate(c)}
                        >
                          <Trash2 className="h-3.5 w-3.5" aria-hidden />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
      {view === "results" && (
        <div className="flex flex-col gap-4">
          {manage && <ImportPanel exam={exam} onImported={refresh} />}
          {sheet.data && <SheetTable sheet={sheet.data} />}
        </div>
      )}
    </Card>
  );
}

const FIELDS = [
  "roll_no",
  "registration_no",
  "subject",
  "obtained",
  "grade",
  "status",
  "remarks",
] as const;

function ImportPanel({
  exam,
  onImported,
}: {
  exam: BoardExamView;
  onImported: () => Promise<void>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [parsed, setParsed] = useState<BoardParsedFile | null>(null);
  const [mapping, setMapping] = useState<Record<string, number>>({});
  const [layout, setLayout] = useState<BoardImportLayout>("wide");
  const [preview, setPreview] = useState<BoardImportPreview | null>(null);
  const [lastBatch, setLastBatch] = useState<string | null>(null);
  const [busy, setBusy] = useState<"parse" | "preview" | "apply" | "rollback" | null>(null);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const options = useMemo(() => {
    const base = [
      { value: "", label: t("boards.results.ignore") },
      ...FIELDS.map((f) => ({ value: f, label: t(`boards.results.field.${f}`) })),
    ];
    if (layout === "wide") {
      for (const s of exam.subjects) {
        base.push({ value: `marks:${s.id}`, label: t("boards.results.marksOf", { name: s.name }) });
        base.push({ value: `grade:${s.id}`, label: t("boards.results.gradeOf", { name: s.name }) });
      }
    }
    return base;
  }, [exam.subjects, layout, t]);
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
      const p = await api.parseBoardResults(exam.id, { file_name: file.name, content_base64 });
      setParsed(p);
      setMapping(p.mapping);
      setLayout(p.layout);
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
    layout,
    save_mapping: true,
  });
  const check = async () => {
    setBusy("preview");
    try {
      setPreview(await api.previewBoardResults(exam.id, body()));
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };
  const apply = async () => {
    setBusy("apply");
    try {
      const res = await api.applyBoardResults(exam.id, body());
      setLastBatch(res.batch_id);
      toast.push({
        kind: "success",
        title: t("boards.results.applied", {
          results: res.summary.results,
          candidates: res.summary.candidates,
        }),
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
      toast.push({ kind: "success", title: t("boards.results.rolledBack") });
      await onImported();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(null);
    }
  };
  return (
    <div className="flex flex-col gap-3 rounded border border-border p-3">
      <div className="flex flex-wrap items-end gap-3">
        <label className="inline-flex cursor-pointer items-center gap-2 rounded border border-border bg-card px-2.5 py-1.5 text-sm font-medium text-text hover:bg-hover">
          <Upload className="h-4 w-4" aria-hidden />
          {t("boards.results.import")}
          <input
            type="file"
            accept=".xlsx,.xlsm,.xls,.csv"
            className="sr-only"
            aria-label={t("boards.results.file")}
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
            {t("boards.results.rollback")}
          </Button>
        )}
      </div>
      {parsed && (
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-end gap-3">
            <Select
              label={t("boards.results.layout")}
              value={layout}
              onChange={(e) => {
                setLayout(e.target.value as BoardImportLayout);
                setPreview(null);
              }}
              options={(["wide", "long"] as const).map((l) => ({
                value: l,
                label: t(`boards.results.layout.${l}`),
              }))}
              className="w-80"
            />
            {parsed.mapping_saved && (
              <span className="text-xs text-muted">{t("boards.results.mappingSaved")}</span>
            )}
          </div>
          <div>
            <div className="mb-1 text-xs font-medium text-text">{t("boards.results.mapping")}</div>
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
                  className="w-52"
                />
              ))}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" loading={busy === "preview"} onClick={() => void check()}>
              {t("boards.results.preview")}
            </Button>
            <Button
              loading={busy === "apply"}
              disabled={!preview || preview.summary.errors > 0 || preview.summary.results === 0}
              onClick={() => void apply()}
            >
              {t("boards.results.apply")}
            </Button>
            {preview && (
              <span className="text-xs text-muted">
                {t("boards.results.summary", {
                  rows: preview.summary.rows,
                  candidates: preview.summary.candidates,
                  results: preview.summary.results,
                  replaced: preview.summary.replaced,
                  errors: preview.summary.errors,
                })}
              </span>
            )}
          </div>
          {preview && preview.summary.errors > 0 && (
            <p role="alert" className="text-sm text-danger">
              {t("boards.results.blocked")}
            </p>
          )}
          {preview && preview.unmapped.length > 0 && (
            <p className="text-xs text-muted">
              {t("boards.results.unmapped", { columns: preview.unmapped.join(", ") })}
            </p>
          )}
          {preview && (
            <table className="w-full text-xs">
              <thead className="bg-hover uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-2 py-1 text-start">#</th>
                  <th className="px-2 py-1 text-start">{t("boards.candidates.rollNo")}</th>
                  <th className="px-2 py-1 text-start">{t("boards.col.student")}</th>
                  <th className="px-2 py-1 text-start">{t("boards.results")}</th>
                </tr>
              </thead>
              <tbody>
                {preview.rows.map((r) => (
                  <tr
                    key={r.row_no}
                    className={`border-t border-border ${r.errors.length ? "bg-danger/10" : ""}`}
                  >
                    <td className="px-2 py-1">{r.row_no}</td>
                    <td className="px-2 py-1">{r.roll_no}</td>
                    <td className="px-2 py-1">{r.candidate_name ?? "—"}</td>
                    <td className="px-2 py-1">
                      {r.errors.length > 0 ? (
                        <span className="text-danger">{r.errors.join("; ")}</span>
                      ) : (
                        r.results
                          .map(
                            (x) =>
                              `${x.subject}: ${x.obtained ?? t(`boards.status.${x.status}`)}${x.grade ? ` (${x.grade})` : ""}${x.replaces ? ` · ${t("boards.results.replaces")}` : ""}`,
                          )
                          .join(" · ")
                      )}
                      {r.warnings.length > 0 && (
                        <span className="ms-1 text-warning">{r.warnings.join("; ")}</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </div>
  );
}

function SheetTable({ sheet }: { sheet: BoardResultSheet }) {
  const { t } = useI18n();
  if (sheet.rows.length === 0) return <EmptyState title={t("boards.results.none")} />;
  return (
    <div className="overflow-x-auto">
      <div className="mb-1 text-xs text-muted">
        {sheet.pass_percent !== null
          ? t("boards.results.passRate", {
              passed: sheet.passed,
              appeared: sheet.with_results,
              percent: num(sheet.pass_percent),
            })
          : t("boards.results.none")}
      </div>
      <table className="w-full text-sm" aria-label={t("boards.results.sheet")}>
        <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
          <tr>
            <th className="px-2 py-1 text-start">{t("boards.candidates.rollNo")}</th>
            <th className="px-2 py-1 text-start">{t("boards.col.student")}</th>
            {sheet.exam.subjects.map((s) => (
              <th key={s.id} className="px-2 py-1 text-end">
                {s.name} /{num(s.max_marks)}
              </th>
            ))}
            <th className="px-2 py-1 text-end">{t("boards.results.total")}</th>
            <th className="px-2 py-1 text-end">{t("boards.results.percent")}</th>
            <th className="px-2 py-1 text-start">{t("boards.results.grade")}</th>
            <th className="px-2 py-1 text-start">{t("boards.results.result")}</th>
            <th className="px-2 py-1 text-end">{t("boards.results.position")}</th>
          </tr>
        </thead>
        <tbody>
          {sheet.rows.map((r) => (
            <tr key={r.candidate.id} className="border-t border-border">
              <td className="px-2 py-1">{r.candidate.roll_no ?? "—"}</td>
              <td className="px-2 py-1">{r.candidate.full_name}</td>
              {r.cells.map((c) => (
                <td key={c.board_subject_id} className="px-2 py-1 text-end">
                  {c.status === null
                    ? ""
                    : c.status === "absent" || c.status === "withheld"
                      ? t(`boards.status.${c.status}`)
                      : `${c.obtained ? num(c.obtained) : ""}${c.grade ? ` (${c.grade})` : ""}`}
                </td>
              ))}
              <td className="px-2 py-1 text-end">
                {r.status === "pending" ? "" : `${num(r.total)}/${num(r.max)}`}
              </td>
              <td className="px-2 py-1 text-end">{r.percent ? num(r.percent) : ""}</td>
              <td className="px-2 py-1">{r.grade ?? ""}</td>
              <td className="px-2 py-1">{t(`boards.status.${r.status}`)}</td>
              <td className="px-2 py-1 text-end">{r.position ?? ""}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
