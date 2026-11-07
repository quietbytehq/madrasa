import { useState } from "react";
import { api } from "@/api/endpoints";
import type {
  AbsenceBody,
  CoverNeed,
  PeriodRef,
  SubstitutionsView,
  TimetableSetup,
} from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { Hint, today } from "./common";

interface Props {
  setup: TimetableSetup;
  manage: boolean;
}

/** Absences, cover suggestions for a day, the covers count and the cover sheet. */
export function SubstitutionsTab({ setup, manage }: Props) {
  const { t, locale, formatDate } = useI18n();
  const toast = useToast();
  const [date, setDate] = useState(today());
  const view = useAsync(
    () => api.substitutions(date, setup.academic_year.id),
    [date, setup.academic_year.id],
  );
  const data: SubstitutionsView | null = view.data;
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const [sheetFrom, setSheetFrom] = useState(today());
  const [sheetTo, setSheetTo] = useState(today());
  const act = async (f: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    try {
      await f();
      await view.reload();
      if (done) toast.push({ kind: "success", title: done });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const decide = (need: CoverNeed, value: string) => {
    if (value === "") {
      void act(() => api.clearCover(need.absenceId, need.date, need.lessonId, need.number));
      return;
    }
    void act(
      () =>
        api.setCover({
          absence_id: need.absenceId,
          date: need.date,
          lesson_id: need.lessonId,
          number: need.number,
          cover_staff_id: value === "none" ? null : value,
        }),
      t("tt.subs.cover.saved"),
    );
  };
  const many = (data?.shifts ?? []).filter((s) => s.enabled).length > 1;
  return (
    <div className="flex flex-col gap-4">
      <Card
        actions={
          manage && (
            <Button size="sm" onClick={() => setAdding(true)}>
              {t("tt.subs.absence.add")}
            </Button>
          )
        }
      >
        <Hint>{t("tt.subs.hint")}</Hint>
        <div className="flex flex-wrap items-end gap-3">
          <Input
            label={t("tt.subs.date")}
            type="date"
            value={date}
            onChange={(e) => e.target.value && setDate(e.target.value)}
            className="w-44"
          />
          {data && (
            <div className="pb-2 text-sm">
              <span className="font-medium">{data.day.title}</span>
              {!data.day.working && (
                <Badge tone="neutral" className="ms-2">
                  {t("tt.subs.notWorking")}
                </Badge>
              )}
              {!data.day.hasTimetable && (
                <Badge tone="warning" className="ms-2">
                  {t("tt.subs.noTimetable")}
                </Badge>
              )}
            </div>
          )}
        </div>
      </Card>
      {view.loading && !data ? (
        <div className="flex justify-center py-10">
          <Spinner size="lg" />
        </div>
      ) : (
        data && (
          <>
            <Card title={t("tt.subs.needs")}>
              {data.day.needs.length === 0 ? (
                <EmptyState title={t("tt.subs.needs.empty")} />
              ) : (
                <Table<CoverNeed>
                  rows={data.day.needs}
                  rowKey={(n) => `${n.absenceId}-${n.lessonId}-${n.number}`}
                  pageSize={0}
                  columns={[
                    {
                      key: "period",
                      header: t("tt.period"),
                      render: (n) => (
                        <span>
                          {many && `${n.shiftName} · `}
                          {n.period}
                          {n.length > 1 && `–${n.period + n.length - 1}`}
                          <span className="ms-1 text-xs text-muted">
                            {n.start}–{n.end}
                          </span>
                        </span>
                      ),
                    },
                    { key: "classes", header: t("tt.lesson.sections"), render: (n) => n.classes },
                    { key: "subject", header: t("tt.lesson.subject"), render: (n) => n.subject },
                    { key: "absent", header: t("tt.subs.absent"), render: (n) => n.absentName },
                    {
                      key: "cover",
                      header: t("tt.subs.cover"),
                      render: (n) =>
                        manage ? (
                          <div className="flex items-center gap-2">
                            <Select
                              aria-label={`${t("tt.subs.cover")} ${n.classes} ${n.period}`}
                              value={n.noCover ? "none" : (n.coverTeacher ?? "")}
                              disabled={busy}
                              onChange={(e) => decide(n, e.target.value)}
                              options={[
                                { value: "", label: t("tt.subs.cover.pick") },
                                { value: "none", label: t("tt.subs.cover.none") },
                                ...n.candidates.map((c) => ({
                                  value: c.teacherId,
                                  disabled: !c.suitable,
                                  label: `${c.name}${c.sameSubject ? ` · ${t("tt.subs.candidate.same")}` : ""} · ${t("tt.subs.candidate.covers", { n: c.covers })}${c.suitable ? "" : ` · ${c.reasons[0] ?? ""}`}`,
                                })),
                              ]}
                              className="w-72"
                            />
                            {(n.coverTeacher || n.noCover) && (
                              <Button
                                size="sm"
                                variant="ghost"
                                disabled={busy}
                                onClick={() => decide(n, "")}
                              >
                                {t("tt.subs.cover.clear")}
                              </Button>
                            )}
                          </div>
                        ) : n.noCover ? (
                          t("tt.subs.cover.none")
                        ) : (
                          (n.coverName ?? <Badge tone="warning">{t("tt.subs.cover.notYet")}</Badge>)
                        ),
                    },
                    { key: "room", header: t("tt.room.name"), render: (n) => n.room || "—" },
                  ]}
                />
              )}
            </Card>
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <Card title={t("tt.subs.absences")}>
                {data.absences.length === 0 ? (
                  <EmptyState title={t("tt.subs.absences.empty")} />
                ) : (
                  <ul className="flex flex-col gap-1 text-sm">
                    {data.absences.slice(0, 30).map((a) => (
                      <li
                        key={a.id}
                        className="flex items-center gap-2 rounded border border-border px-2 py-1"
                      >
                        <span className="font-medium">{a.staff_name}</span>
                        <span className="text-muted">
                          {formatDate(a.from_date)}
                          {a.to_date !== a.from_date && ` – ${formatDate(a.to_date)}`}
                          {a.periods.length > 0 &&
                            ` · ${a.periods.map((p) => p.period).join(", ")}`}
                        </span>
                        {a.note && <span className="text-xs text-muted">{a.note}</span>}
                        {manage && (
                          <Button
                            size="sm"
                            variant="ghost"
                            className="ms-auto"
                            disabled={busy}
                            onClick={() => {
                              if (window.confirm(t("tt.subs.absence.deleteConfirm")))
                                void act(() => api.deleteAbsence(a.id));
                            }}
                          >
                            {t("tt.subs.absence.delete")}
                          </Button>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </Card>
              <Card title={t("tt.subs.counts")}>
                <div className="mb-2 text-sm">
                  <Badge tone="info">{data.counts.total}</Badge>
                </div>
                <ul className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
                  {data.counts.teachers
                    .filter((x) => x.covers > 0)
                    .map((x) => (
                      <li key={x.teacherId} className="flex justify-between">
                        <span>{x.name}</span>
                        <span className="tabular-nums">{x.covers}</span>
                      </li>
                    ))}
                </ul>
              </Card>
            </div>
            <Card title={t("tt.subs.sheet")}>
              <div className="flex flex-wrap items-end gap-3">
                <Input
                  label={t("tt.subs.sheet.from")}
                  type="date"
                  value={sheetFrom}
                  onChange={(e) => setSheetFrom(e.target.value)}
                  className="w-44"
                />
                <Input
                  label={t("tt.subs.sheet.to")}
                  type="date"
                  value={sheetTo}
                  onChange={(e) => setSheetTo(e.target.value)}
                  className="w-44"
                />
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() =>
                    void api.downloadCoverSheet(sheetFrom, sheetTo, locale).catch((err) =>
                      toast.push({
                        kind: "error",
                        title: errorMessage(err, t("app.error.generic")),
                      }),
                    )
                  }
                >
                  {t("tt.subs.sheet.print")}
                </Button>
              </div>
            </Card>
          </>
        )
      )}
      {adding && data && (
        <AbsenceDialog
          view={data}
          date={date}
          busy={busy}
          onClose={() => setAdding(false)}
          onSave={(body) =>
            void act(() =>
              api.createAbsence({ ...body, academic_year_id: setup.academic_year.id }),
            ).then(() => setAdding(false))
          }
        />
      )}
    </div>
  );
}

function AbsenceDialog({
  view,
  date,
  busy,
  onClose,
  onSave,
}: {
  view: SubstitutionsView;
  date: string;
  busy: boolean;
  onClose: () => void;
  onSave: (body: AbsenceBody) => void;
}) {
  const { t } = useI18n();
  const [staff, setStaff] = useState(view.teachers[0]?.id ?? "");
  const [from, setFrom] = useState(date);
  const [to, setTo] = useState(date);
  const [periods, setPeriods] = useState<PeriodRef[]>([]);
  const [note, setNote] = useState("");
  const shifts = view.shifts.filter((s) => s.enabled);
  const toggle = (shift_id: string, period: number) =>
    setPeriods((p) =>
      p.some((x) => x.shift_id === shift_id && x.period === period)
        ? p.filter((x) => !(x.shift_id === shift_id && x.period === period))
        : [...p, { shift_id, period }],
    );
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("tt.subs.absence.add")}
      footer={
        <div className="flex gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!staff || !from}
            onClick={() =>
              onSave({ staff_id: staff, from_date: from, to_date: to || from, periods, note })
            }
          >
            {t("action.save")}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <Select
          label={t("tt.subs.absence.teacher")}
          value={staff}
          onChange={(e) => setStaff(e.target.value)}
          options={view.teachers.map((x) => ({ value: x.id, label: x.name }))}
        />
        <div className="flex gap-3">
          <Input
            label={t("tt.subs.absence.from")}
            type="date"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
          />
          <Input
            label={t("tt.subs.absence.to")}
            type="date"
            value={to}
            onChange={(e) => setTo(e.target.value)}
          />
        </div>
        <div>
          <div className="mb-1 text-xs font-medium text-muted">{t("tt.subs.absence.periods")}</div>
          {shifts.map((s) => (
            <div key={s.id} className="mb-1 flex flex-wrap items-center gap-1">
              {shifts.length > 1 && <span className="me-1 text-xs text-muted">{s.name}</span>}
              {s.period_times.map((_, i) => {
                const on = periods.some((x) => x.shift_id === s.id && x.period === i + 1);
                return (
                  <button
                    key={i}
                    type="button"
                    aria-pressed={on}
                    onClick={() => toggle(s.id, i + 1)}
                    className={`h-7 w-8 rounded border text-xs ${on ? "border-blue bg-selection text-blue" : "border-border"}`}
                  >
                    {i + 1}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
        <Input
          label={t("tt.subs.absence.note")}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </div>
    </Dialog>
  );
}
