import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { History } from "lucide-react";
import { api } from "@/api/endpoints";
import type { AttendanceStatus, RosterRow, SheetHistoryEntry, SheetView } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { DayKindNotice, STATUSES, STATUS_KEYS, statusLabel, todayIso } from "../common";

type Mark = { status: AttendanceStatus; note: string };

/** One-screen marking per section. Everyone is present; the teacher marks the exceptions. */
export function MarkTab({ initialSection }: { initialSection?: string }) {
  const { t } = useI18n();
  const [date, setDate] = useState(todayIso());
  const [sectionId, setSectionId] = useState(initialSection ?? "");
  const status = useAsync(() => api.attendanceStatus(date), [date]);
  const sections = useMemo(() => status.data?.sections ?? [], [status.data]);
  useEffect(() => {
    if (!sectionId && sections.length > 0) setSectionId(sections[0]!.section_id);
  }, [sections, sectionId]);
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <Select
          label={t("attendance.section")}
          value={sectionId}
          onChange={(e) => setSectionId(e.target.value)}
          options={sections.map((s) => ({
            value: s.section_id,
            label: `${s.class_name}-${s.section_name}${s.sheet_id ? " ✓" : ""}`,
          }))}
          className="w-48"
        />
        <Input
          label={t("attendance.date")}
          type="date"
          value={date}
          max={todayIso()}
          onChange={(e) => e.target.value && setDate(e.target.value)}
          className="w-44"
        />
      </div>
      {status.loading && !status.data ? (
        <div className="flex justify-center py-10">
          <Spinner />
        </div>
      ) : sections.length === 0 ? (
        <Card>
          <EmptyState title={t("attendance.mark.noSections")} />
        </Card>
      ) : sectionId ? (
        <Sheet
          key={`${sectionId}-${date}`}
          sectionId={sectionId}
          date={date}
          onSaved={() => void status.reload()}
        />
      ) : null}
    </div>
  );
}

function Sheet({
  sectionId,
  date,
  onSaved,
}: {
  sectionId: string;
  date: string;
  onSaved: () => void;
}) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const sheet = useAsync(() => api.attendanceSheet(sectionId, date), [sectionId, date]);
  // Local edits on top of the loaded roster; a student without an override shows the
  // stored entry, or "present" when the sheet does not exist yet.
  const [marks, setMarks] = useState<Record<string, Mark>>({});
  const [busy, setBusy] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const rowRefs = useRef<Array<HTMLTableRowElement | null>>([]);
  const students = useMemo(() => sheet.data?.students ?? [], [sheet.data]);
  const markOf = useMemo(
    () => (s: RosterRow) =>
      marks[s.student_id] ?? { status: s.status ?? ("present" as const), note: s.note ?? "" },
    [marks],
  );
  const summary = useMemo(() => {
    const c = { present: 0, absent: 0, late: 0, leave: 0 };
    for (const s of students) c[markOf(s).status] += 1;
    return c;
  }, [students, markOf]);

  if (sheet.loading && !sheet.data) {
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  }
  if (sheet.error || !sheet.data)
    return <EmptyState title={errorMessage(sheet.error, t("common.error.load"))} />;
  const v: SheetView = sheet.data;
  const set = (id: string, patch: Partial<Mark>) => {
    const row = students.find((s) => s.student_id === id);
    if (!row) return;
    setMarks((m) => ({ ...m, [id]: { ...markOf(row), ...patch } }));
  };
  const focusRow = (i: number) => rowRefs.current[i]?.focus();

  const onKey = (e: KeyboardEvent<HTMLTableRowElement>, i: number, id: string) => {
    if ((e.target as HTMLElement).tagName === "INPUT") {
      if (e.key === "Enter" || e.key === "ArrowDown") {
        e.preventDefault();
        focusRow(i + 1);
      }
      return;
    }
    const key = e.key.toLowerCase();
    if (key === "arrowdown" || key === "enter") {
      e.preventDefault();
      focusRow(i + 1);
    } else if (key === "arrowup") {
      e.preventDefault();
      focusRow(i - 1);
    } else if (STATUS_KEYS[key] && v.can_edit) {
      e.preventDefault();
      set(id, { status: STATUS_KEYS[key] });
    }
  };

  const save = async () => {
    setBusy(true);
    try {
      const entries = students
        .map((s) => [s.student_id, markOf(s)] as const)
        .filter(([, m]) => m.status !== "present" || m.note.trim())
        .map(([student_id, m]) => ({
          student_id,
          status: m.status,
          note: m.note.trim() || undefined,
        }));
      const next = await api.saveAttendanceSheet({ section_id: sectionId, date, entries });
      setMarks({});
      sheet.setData(next);
      toast.push({ kind: "success", title: t("attendance.mark.saved") });
      onSaved();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      padded={false}
      title={`${t("common.class")} ${v.section.class_name}-${v.section.section_name}`}
      actions={
        <div className="flex items-center gap-2">
          {v.sheet && (
            <Button
              size="sm"
              variant="ghost"
              icon={<History className="h-4 w-4" aria-hidden />}
              onClick={() => setShowHistory(true)}
            >
              {t("attendance.history")}
            </Button>
          )}
          {v.can_edit && (
            <Button
              size="sm"
              variant="secondary"
              onClick={() =>
                setMarks(
                  Object.fromEntries(
                    students.map((s) => [s.student_id, { status: "present" as const, note: "" }]),
                  ),
                )
              }
            >
              {t("attendance.mark.allPresent")}
            </Button>
          )}
          {v.can_edit && (
            <Button size="sm" loading={busy} onClick={() => void save()}>
              {v.is_correction ? t("attendance.mark.correct") : t("attendance.mark.submit")}
            </Button>
          )}
        </div>
      }
    >
      <div className="flex flex-col gap-2 border-b border-border px-4 py-2 text-xs text-muted">
        <DayKindNotice kind={v.day_kind} t={t} />
        {v.sheet && (
          <p>
            {t("attendance.mark.submitted", {
              name: v.marked_by_name ?? "—",
              time: formatDate(v.sheet.submitted_at),
            })}{" "}
            {v.is_correction && !v.can_edit && (
              <span className="text-text">{t("attendance.mark.locked")}</span>
            )}
          </p>
        )}
        {v.is_correction && v.can_edit && (
          <p role="note" className="text-warning">
            {t("attendance.mark.correctionNote")}
          </p>
        )}
        {!v.sheet && v.can_edit && <p>{t("attendance.mark.everyonePresent")}</p>}
        <p>{t("attendance.keys.hint")}</p>
        <p className="font-medium text-text" data-testid="mark-summary">
          {t("attendance.mark.summary", summary)}
        </p>
      </div>
      {v.students.length === 0 ? (
        <EmptyState title={t("attendance.mark.noStudents")} />
      ) : (
        <table className="w-full text-sm">
          <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="w-14 px-3 py-2 text-start">{t("attendance.mark.rollNo")}</th>
              <th className="px-3 py-2 text-start">{t("students.fullName")}</th>
              <th className="px-3 py-2 text-start">{t("common.status")}</th>
              <th className="px-3 py-2 text-start">{t("attendance.note")}</th>
            </tr>
          </thead>
          <tbody>
            {v.students.map((s, i) => {
              const m = markOf(s);
              return (
                <tr
                  key={s.student_id}
                  ref={(el) => {
                    rowRefs.current[i] = el;
                  }}
                  tabIndex={0}
                  data-testid={`row-${s.student_id}`}
                  onKeyDown={(e) => onKey(e, i, s.student_id)}
                  className={cn(
                    "border-t border-border outline-none focus:bg-selection",
                    m.status === "absent" && "bg-danger-bg/40",
                  )}
                >
                  <td className="px-3 py-1 tabular-nums text-muted">{s.roll_no ?? "—"}</td>
                  <td className="px-3 py-1">
                    <span className="font-medium">{s.full_name}</span>{" "}
                    <span className="text-xs text-muted">{s.student_number}</span>
                  </td>
                  <td className="px-3 py-1">
                    <div
                      role="radiogroup"
                      aria-label={`${s.full_name}: ${t("common.status")}`}
                      className="flex gap-1"
                    >
                      {STATUSES.map((st) => (
                        <button
                          key={st}
                          type="button"
                          role="radio"
                          aria-checked={m.status === st}
                          disabled={!v.can_edit}
                          tabIndex={-1}
                          onClick={() => set(s.student_id, { status: st })}
                          className={cn(
                            "rounded border px-2 py-0.5 text-xs",
                            m.status === st
                              ? st === "present"
                                ? "border-success bg-success-bg text-success"
                                : st === "absent"
                                  ? "border-danger bg-danger-bg text-danger"
                                  : st === "late"
                                    ? "border-warning bg-warning-bg text-warning"
                                    : "border-blue bg-selection text-blue"
                              : "border-border text-muted hover:bg-hover",
                            !v.can_edit && "cursor-default",
                          )}
                        >
                          {statusLabel(t, st)}
                        </button>
                      ))}
                    </div>
                  </td>
                  <td className="px-3 py-1">
                    {m.status !== "present" || m.note ? (
                      <input
                        aria-label={`${s.full_name}: ${t("attendance.note")}`}
                        value={m.note}
                        disabled={!v.can_edit}
                        onChange={(e) => set(s.student_id, { note: e.target.value })}
                        className="h-7 w-full rounded border border-border bg-card px-2 text-xs"
                      />
                    ) : null}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      {showHistory && (
        <HistoryDialog sectionId={sectionId} date={date} onClose={() => setShowHistory(false)} />
      )}
    </Card>
  );
}

function HistoryDialog({
  sectionId,
  date,
  onClose,
}: {
  sectionId: string;
  date: string;
  onClose: () => void;
}) {
  const { t, formatDate } = useI18n();
  const history = useAsync(() => api.attendanceSheetHistory(sectionId, date), [sectionId, date]);
  const describe = (h: SheetHistoryEntry) => {
    if (h.entity === "attendance_sheet")
      return h.action === "create"
        ? t("attendance.history.submitted")
        : t("attendance.history.corrected", {
            count: Number(h.new_value?.corrected_entries ?? 0),
          });
    const to = statusLabel(t, (h.new_value?.status as AttendanceStatus) ?? "present");
    if (!h.old_value) return t("attendance.history.added", { student: h.student_name ?? "", to });
    return t("attendance.history.entry", {
      student: h.student_name ?? "",
      from: statusLabel(t, (h.old_value.status as AttendanceStatus) ?? "present"),
      to,
    });
  };
  return (
    <Dialog open onClose={onClose} title={t("attendance.history")} size="md">
      {history.loading && !history.data ? (
        <Spinner />
      ) : (history.data?.length ?? 0) === 0 ? (
        <EmptyState title={t("attendance.history.empty")} />
      ) : (
        <ul className="divide-y divide-border text-sm">
          {history.data!.map((h, i) => (
            <li key={i} className="flex flex-wrap gap-x-3 py-2">
              <span className="text-muted">{formatDate(h.at)}</span>
              <span className="font-medium">{h.actor_name ?? "—"}</span>
              <span>{describe(h)}</span>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}
