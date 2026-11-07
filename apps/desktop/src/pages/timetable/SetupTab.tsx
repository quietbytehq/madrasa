import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type {
  Named,
  Room,
  RoomBody,
  RuleKind,
  SchoolRules,
  ShiftBody,
  TeacherLimits,
  TimeOffCell,
  TimetableRule,
  TimetableSetup,
  TimetableShift,
} from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/useAsync";
import { CheckList, DAYS, Hint, dayLabel, shiftDaysSummary } from "./common";
import { TimeOffEditor } from "./TimeOffEditor";

interface Props {
  setup: TimetableSetup;
  manage: boolean;
  reload: () => Promise<void>;
}

/** Setup: shifts, rooms, school rules and per-entity availability. */
export function SetupTab({ setup, manage, reload }: Props) {
  return (
    <div className="flex flex-col gap-4">
      <ShiftsCard setup={setup} manage={manage} reload={reload} />
      <RoomsCard setup={setup} manage={manage} reload={reload} />
      <SchoolRulesCard setup={setup} manage={manage} reload={reload} />
      <EntityRulesCard setup={setup} manage={manage} reload={reload} />
    </div>
  );
}

// ---- shifts ---------------------------------------------------------------------------------

function blankShift(): ShiftBody {
  return {
    name: "",
    enabled: true,
    days: DAYS.map((d) => ({ working: d < 5, periods: d < 5 ? 7 : 0 })),
    period_times: Array.from({ length: 7 }, (_, i) => {
      const start = 8 * 60 + i * 40 + (i >= 3 ? 20 : 0);
      const fmt = (m: number) =>
        `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
      return { start: fmt(start), end: fmt(start + 40) };
    }),
    breaks: [{ after_period: 3, name: "Break", cannot_cross: true }],
  };
}

function ShiftsCard({ setup, manage, reload }: Props) {
  const { t } = useI18n();
  const toast = useToast();
  const [editing, setEditing] = useState<{ id: string | null; body: ShiftBody } | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async (f: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    try {
      await f();
      await reload();
      if (done) toast.push({ kind: "success", title: done });
      setEditing(null);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const edit = (s: TimetableShift) =>
    setEditing({
      id: s.id,
      body: {
        name: s.name,
        enabled: s.enabled,
        days: s.days,
        period_times: s.period_times,
        breaks: s.breaks,
        position: s.position,
      },
    });
  return (
    <Card
      title={t("tt.shifts")}
      actions={
        manage && (
          <div className="flex gap-2">
            {setup.shifts.length === 0 && (
              <Button
                size="sm"
                variant="secondary"
                loading={busy}
                onClick={() => void run(() => api.createDefaultShift(setup.academic_year.id))}
              >
                {t("tt.shifts.addDefault")}
              </Button>
            )}
            <Button size="sm" onClick={() => setEditing({ id: null, body: blankShift() })}>
              {t("tt.shifts.add")}
            </Button>
          </div>
        )
      }
    >
      <Hint>{t("tt.shifts.hint")}</Hint>
      {setup.shifts.length === 0 ? (
        <EmptyState title={t("tt.shifts.empty")} />
      ) : (
        <Table<TimetableShift>
          rows={setup.shifts}
          rowKey={(s) => s.id}
          onRowClick={manage ? edit : undefined}
          columns={[
            { key: "name", header: t("tt.shift.name"), render: (s) => s.name },
            {
              key: "enabled",
              header: t("tt.shift.enabled"),
              render: (s) => (
                <Badge tone={s.enabled ? "success" : "neutral"}>{s.enabled ? "✓" : "—"}</Badge>
              ),
            },
            { key: "days", header: t("tt.shift.days"), render: (s) => shiftDaysSummary(t, s) },
            {
              key: "periods",
              header: t("tt.shift.periods"),
              render: (s) =>
                `${t("tt.shift.periodCount", { n: s.period_times.length })} · ${s.period_times[0]?.start ?? ""}–${s.period_times[s.period_times.length - 1]?.end ?? ""}`,
            },
            {
              key: "breaks",
              header: t("tt.shift.breaks"),
              render: (s) =>
                s.breaks.map((b) => `${b.name} (${b.after_period})`).join(", ") ||
                t("tt.shift.break.none"),
            },
          ]}
        />
      )}
      {editing && (
        <ShiftDialog
          initial={editing.body}
          isNew={editing.id === null}
          busy={busy}
          onClose={() => setEditing(null)}
          onSave={(body) =>
            void run(() =>
              editing.id
                ? api.updateShift(editing.id, body)
                : api.createShift({ ...body, academic_year_id: setup.academic_year.id }),
            )
          }
          onDelete={
            editing.id
              ? () => {
                  if (window.confirm(t("tt.shift.deleteConfirm")))
                    void run(() => api.deleteShift(editing.id!));
                }
              : undefined
          }
        />
      )}
    </Card>
  );
}

function ShiftDialog({
  initial,
  isNew,
  busy,
  onClose,
  onSave,
  onDelete,
}: {
  initial: ShiftBody;
  isNew: boolean;
  busy: boolean;
  onClose: () => void;
  onSave: (body: ShiftBody) => void;
  onDelete?: () => void;
}) {
  const { t } = useI18n();
  const [body, setBody] = useState<ShiftBody>(initial);
  const n = body.period_times.length;
  const breakAfter = body.breaks[0]?.after_period ?? 0;
  const setDay = (i: number, patch: Partial<{ working: boolean; periods: number }>) =>
    setBody({
      ...body,
      days: body.days.map((d, j) => (j === i ? { ...d, ...patch } : d)),
    });
  return (
    <Dialog
      open
      onClose={onClose}
      title={isNew ? t("tt.shifts.add") : t("tt.shift.edit")}
      size="lg"
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          <div>
            {onDelete && (
              <Button variant="danger" size="sm" onClick={onDelete} disabled={busy}>
                {t("tt.shift.delete")}
              </Button>
            )}
          </div>
          <div className="flex gap-2">
            <Button variant="secondary" onClick={onClose}>
              {t("action.cancel")}
            </Button>
            <Button loading={busy} onClick={() => onSave(body)} disabled={!body.name.trim()}>
              {t("action.save")}
            </Button>
          </div>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-end gap-3">
          <Input
            label={t("tt.shift.name")}
            value={body.name}
            onChange={(e) => setBody({ ...body, name: e.target.value })}
            className="w-48"
          />
          <label className="flex items-center gap-2 pb-2 text-sm">
            <input
              type="checkbox"
              checked={body.enabled}
              onChange={(e) => setBody({ ...body, enabled: e.target.checked })}
            />
            {t("tt.shift.enabled")}
          </label>
        </div>
        <div>
          <div className="mb-1 text-xs font-medium text-muted">{t("tt.shift.days")}</div>
          <div className="grid grid-cols-7 gap-1 text-xs">
            {DAYS.map((d) => (
              <div key={d} className="rounded border border-border p-1 text-center">
                <label className="flex items-center justify-center gap-1">
                  <input
                    type="checkbox"
                    checked={body.days[d]?.working ?? false}
                    onChange={(e) =>
                      setDay(d, {
                        working: e.target.checked,
                        periods: e.target.checked ? body.days[d]?.periods || n : 0,
                      })
                    }
                  />
                  {dayLabel(t, d, true)}
                </label>
                <input
                  type="number"
                  min={1}
                  max={n}
                  aria-label={`${dayLabel(t, d)} ${t("tt.shift.periodsOnDay")}`}
                  disabled={!body.days[d]?.working}
                  value={body.days[d]?.working ? body.days[d].periods : ""}
                  onChange={(e) => setDay(d, { periods: Number(e.target.value) })}
                  className="mt-1 w-full rounded border border-border px-1 py-0.5 text-center"
                />
              </div>
            ))}
          </div>
        </div>
        <div>
          <div className="mb-1 flex items-center justify-between">
            <span className="text-xs font-medium text-muted">{t("tt.shift.periods")}</span>
            <div className="flex gap-1">
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  const last = body.period_times[n - 1];
                  const [h = 8, m = 0] = (last?.end ?? "08:00").split(":").map(Number);
                  const start = h * 60 + m;
                  const fmt = (x: number) =>
                    `${String(Math.floor(x / 60) % 24).padStart(2, "0")}:${String(x % 60).padStart(2, "0")}`;
                  setBody({
                    ...body,
                    period_times: [
                      ...body.period_times,
                      { start: fmt(start), end: fmt(start + 40) },
                    ],
                  });
                }}
              >
                {t("tt.shift.addPeriod")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={n <= 1}
                onClick={() =>
                  setBody({
                    ...body,
                    period_times: body.period_times.slice(0, -1),
                    days: body.days.map((d) => ({ ...d, periods: Math.min(d.periods, n - 1) })),
                    breaks: body.breaks.filter((b) => b.after_period < n - 1),
                  })
                }
              >
                {t("tt.shift.removePeriod")}
              </Button>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-x-4 gap-y-1 md:grid-cols-3">
            {body.period_times.map((p, i) => (
              <div key={i} className="flex items-center gap-1 text-xs">
                <span className="w-16 text-muted">{t("tt.shift.period", { n: i + 1 })}</span>
                <input
                  type="time"
                  aria-label={`${t("tt.shift.period", { n: i + 1 })} ${t("tt.shift.start")}`}
                  value={p.start}
                  onChange={(e) =>
                    setBody({
                      ...body,
                      period_times: body.period_times.map((x, j) =>
                        j === i ? { ...x, start: e.target.value } : x,
                      ),
                    })
                  }
                  className="rounded border border-border px-1 py-0.5"
                />
                <input
                  type="time"
                  aria-label={`${t("tt.shift.period", { n: i + 1 })} ${t("tt.shift.end")}`}
                  value={p.end}
                  onChange={(e) =>
                    setBody({
                      ...body,
                      period_times: body.period_times.map((x, j) =>
                        j === i ? { ...x, end: e.target.value } : x,
                      ),
                    })
                  }
                  className="rounded border border-border px-1 py-0.5"
                />
              </div>
            ))}
          </div>
        </div>
        <div className="flex flex-wrap items-end gap-3">
          <Select
            label={t("tt.shift.break.after")}
            value={String(breakAfter)}
            onChange={(e) => {
              const after = Number(e.target.value);
              setBody({
                ...body,
                breaks: after
                  ? [
                      {
                        after_period: after,
                        name: body.breaks[0]?.name || "Break",
                        cannot_cross: true,
                      },
                    ]
                  : [],
              });
            }}
            options={[
              { value: "0", label: t("tt.shift.break.none") },
              ...Array.from({ length: Math.max(0, n - 1) }, (_, i) => ({
                value: String(i + 1),
                label: String(i + 1),
              })),
            ]}
            className="w-40"
          />
          {breakAfter > 0 && (
            <Input
              label={t("tt.shift.break.name")}
              value={body.breaks[0]?.name ?? ""}
              onChange={(e) =>
                setBody({
                  ...body,
                  breaks: [{ after_period: breakAfter, cannot_cross: true, name: e.target.value }],
                })
              }
              className="w-40"
            />
          )}
        </div>
      </div>
    </Dialog>
  );
}

// ---- rooms ----------------------------------------------------------------------------------

function RoomsCard({ setup, manage, reload }: Props) {
  const { t } = useI18n();
  const toast = useToast();
  const [editing, setEditing] = useState<{ id: string | null; body: RoomBody } | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
      await reload();
      setEditing(null);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const body = editing?.body;
  return (
    <Card
      title={t("tt.rooms")}
      actions={
        manage && (
          <Button
            size="sm"
            onClick={() =>
              setEditing({
                id: null,
                body: { name: "", short_name: "", building: "", capacity: null },
              })
            }
          >
            {t("tt.rooms.add")}
          </Button>
        )
      }
    >
      <Hint>{t("tt.rooms.hint")}</Hint>
      {setup.rooms.length === 0 ? (
        <EmptyState title={t("tt.rooms.empty")} />
      ) : (
        <Table<Room>
          rows={setup.rooms}
          rowKey={(r) => r.id}
          onRowClick={
            manage
              ? (r) =>
                  setEditing({
                    id: r.id,
                    body: {
                      name: r.name,
                      short_name: r.short_name,
                      building: r.building,
                      capacity: r.capacity,
                    },
                  })
              : undefined
          }
          columns={[
            { key: "name", header: t("tt.room.name"), sortable: true },
            { key: "short_name", header: t("tt.room.short") },
            { key: "building", header: t("tt.room.building"), sortable: true },
            {
              key: "capacity",
              header: t("tt.room.capacity"),
              align: "end",
              render: (r) => r.capacity ?? "—",
            },
          ]}
        />
      )}
      {editing && body && (
        <Dialog
          open
          onClose={() => setEditing(null)}
          title={editing.id ? t("tt.room.edit") : t("tt.rooms.add")}
          footer={
            <div className="flex w-full items-center justify-between gap-2">
              <div>
                {editing.id && (
                  <Button
                    variant="danger"
                    size="sm"
                    disabled={busy}
                    onClick={() => {
                      if (window.confirm(t("tt.room.deleteConfirm")))
                        void run(() => api.deleteRoom(editing.id!));
                    }}
                  >
                    {t("action.delete")}
                  </Button>
                )}
              </div>
              <div className="flex gap-2">
                <Button variant="secondary" onClick={() => setEditing(null)}>
                  {t("action.cancel")}
                </Button>
                <Button
                  loading={busy}
                  disabled={!body.name.trim()}
                  onClick={() =>
                    void run(() =>
                      editing.id ? api.updateRoom(editing.id, body) : api.createRoom(body),
                    )
                  }
                >
                  {t("action.save")}
                </Button>
              </div>
            </div>
          }
        >
          <div className="grid grid-cols-2 gap-3">
            <Input
              label={t("tt.room.name")}
              value={body.name}
              onChange={(e) => setEditing({ ...editing, body: { ...body, name: e.target.value } })}
            />
            <Input
              label={t("tt.room.short")}
              value={body.short_name ?? ""}
              onChange={(e) =>
                setEditing({ ...editing, body: { ...body, short_name: e.target.value } })
              }
            />
            <Input
              label={t("tt.room.building")}
              value={body.building ?? ""}
              onChange={(e) =>
                setEditing({ ...editing, body: { ...body, building: e.target.value } })
              }
            />
            <Input
              label={t("tt.room.capacity")}
              type="number"
              min={0}
              value={body.capacity ?? ""}
              onChange={(e) =>
                setEditing({
                  ...editing,
                  body: {
                    ...body,
                    capacity: e.target.value === "" ? null : Number(e.target.value),
                  },
                })
              }
            />
          </div>
        </Dialog>
      )}
    </Card>
  );
}

// ---- school rules -----------------------------------------------------------------------------

function SchoolRulesCard({ setup, manage, reload }: Props) {
  const { t } = useI18n();
  const toast = useToast();
  const [rules, setRules] = useState<SchoolRules>(setup.school_rules);
  const [busy, setBusy] = useState(false);
  useEffect(() => setRules(setup.school_rules), [setup.school_rules]);
  const num = (v: string): number | null => (v === "" ? null : Number(v));
  const save = async () => {
    setBusy(true);
    try {
      await api.setTimetableRule({
        academic_year_id: setup.academic_year.id,
        kind: "school",
        limits: rules,
      });
      await reload();
      toast.push({ kind: "success", title: t("tt.rules.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      title={t("tt.rules")}
      actions={
        manage && (
          <Button size="sm" loading={busy} onClick={() => void save()}>
            {t("tt.rules.save")}
          </Button>
        )
      }
    >
      <Hint>{t("tt.rules.hint")}</Hint>
      <div className="flex flex-wrap items-end gap-3">
        <Input
          label={t("tt.rules.maxPerDay")}
          type="number"
          min={1}
          disabled={!manage}
          value={rules.default_max_per_day ?? ""}
          onChange={(e) => setRules({ ...rules, default_max_per_day: num(e.target.value) })}
          className="w-28"
        />
        <Input
          label={t("tt.rules.maxPerWeek")}
          type="number"
          min={1}
          disabled={!manage}
          value={rules.default_max_per_week ?? ""}
          onChange={(e) => setRules({ ...rules, default_max_per_week: num(e.target.value) })}
          className="w-28"
        />
        <Input
          label={t("tt.rules.sameSubject")}
          type="number"
          min={1}
          max={8}
          disabled={!manage}
          value={rules.max_same_subject_per_day}
          onChange={(e) =>
            setRules({ ...rules, max_same_subject_per_day: Number(e.target.value) || 1 })
          }
          className="w-28"
        />
        <Input
          label={t("tt.rules.improve")}
          type="number"
          min={0}
          max={300}
          disabled={!manage}
          value={rules.improve_seconds}
          onChange={(e) => setRules({ ...rules, improve_seconds: Number(e.target.value) || 0 })}
          className="w-28"
        />
        <label className="flex items-center gap-2 pb-2 text-sm">
          <input
            type="checkbox"
            disabled={!manage}
            checked={rules.allow_relaxation}
            onChange={(e) => setRules({ ...rules, allow_relaxation: e.target.checked })}
          />
          {t("tt.rules.relax")}
        </label>
      </div>
    </Card>
  );
}

// ---- per-entity rules -------------------------------------------------------------------------

type EntityKind = Exclude<RuleKind, "school">;

function EntityRulesCard({ setup, manage, reload }: Props) {
  const { t } = useI18n();
  const toast = useToast();
  const [kind, setKind] = useState<EntityKind>("teacher");
  const [entity, setEntity] = useState("");
  const [shiftIds, setShiftIds] = useState<string[]>([]);
  const [homeRoom, setHomeRoom] = useState("");
  const [timeOff, setTimeOff] = useState<TimeOffCell[]>([]);
  const [limits, setLimits] = useState<TeacherLimits>({});
  const [busy, setBusy] = useState(false);
  const items: Named[] =
    kind === "teacher"
      ? setup.teachers
      : kind === "section"
        ? setup.sections
        : setup.rooms.map((r) => ({ id: r.id, name: r.name }));
  const rule: TimetableRule | undefined = setup.rules.find(
    (r) => r.kind === kind && r.entity_id === entity,
  );
  useEffect(() => {
    setShiftIds(rule?.shift_ids ?? []);
    setHomeRoom(rule?.home_room_id ?? "");
    setTimeOff(rule?.time_off ?? []);
    setLimits(kind === "teacher" ? ((rule?.limits as TeacherLimits | undefined) ?? {}) : {});
  }, [rule, kind, entity]);
  const num = (v: string): number | null => (v === "" ? null : Number(v));
  const save = async () => {
    setBusy(true);
    try {
      await api.setTimetableRule({
        academic_year_id: setup.academic_year.id,
        kind,
        entity_id: entity,
        shift_ids: kind === "room" ? [] : shiftIds,
        home_room_id: kind === "section" ? homeRoom || null : null,
        time_off: timeOff,
        limits: kind === "teacher" ? limits : {},
      });
      await reload();
      toast.push({ kind: "success", title: t("tt.rules.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const enabledShifts = setup.shifts.filter((s) => s.enabled);
  return (
    <Card
      title={t(`tt.rules.${kind}`)}
      actions={
        manage &&
        entity && (
          <Button size="sm" loading={busy} onClick={() => void save()}>
            {t("tt.rules.save")}
          </Button>
        )
      }
    >
      <div className="mb-3 flex flex-wrap items-end gap-3">
        <Select
          label={t("tt.view.kind")}
          value={kind}
          onChange={(e) => {
            setKind(e.target.value as EntityKind);
            setEntity("");
          }}
          options={[
            { value: "teacher", label: t("tt.view.teacher") },
            { value: "section", label: t("tt.view.section") },
            { value: "room", label: t("tt.view.room") },
          ]}
          className="w-40"
        />
        <Select
          label={t(`tt.view.${kind}`)}
          value={entity}
          onChange={(e) => setEntity(e.target.value)}
          options={[
            { value: "", label: t("tt.rules.pick") },
            ...items.map((i) => ({ value: i.id, label: i.name })),
          ]}
          className="w-64"
        />
      </div>
      {entity && (
        <div className="flex flex-col gap-3">
          {kind === "teacher" && (
            <>
              <CheckList
                label={t("tt.rules.shifts")}
                items={enabledShifts.map((s) => ({ id: s.id, name: s.name }))}
                value={shiftIds}
                onChange={setShiftIds}
                columns={3}
              />
              <div className="flex flex-wrap gap-3">
                {(
                  [
                    ["max_per_day", "tt.rules.limits.day"],
                    ["max_per_week", "tt.rules.limits.week"],
                    ["max_consecutive", "tt.rules.limits.consecutive"],
                    ["max_days_per_week", "tt.rules.limits.days"],
                  ] as const
                ).map(([key, label]) => (
                  <Input
                    key={key}
                    label={t(label)}
                    type="number"
                    min={1}
                    disabled={!manage}
                    value={limits[key] ?? ""}
                    onChange={(e) => setLimits({ ...limits, [key]: num(e.target.value) })}
                    className="w-32"
                  />
                ))}
              </div>
            </>
          )}
          {kind === "section" && (
            <div className="flex flex-wrap gap-3">
              <Select
                label={t("tt.rules.sectionShift")}
                value={shiftIds[0] ?? ""}
                disabled={!manage}
                onChange={(e) => setShiftIds(e.target.value ? [e.target.value] : [])}
                options={[
                  { value: "", label: enabledShifts[0]?.name ?? "—" },
                  ...enabledShifts.map((s) => ({ value: s.id, label: s.name })),
                ]}
                className="w-48"
              />
              <Select
                label={t("tt.rules.homeRoom")}
                value={homeRoom}
                disabled={!manage}
                onChange={(e) => setHomeRoom(e.target.value)}
                options={[
                  { value: "", label: t("tt.rules.anyRoom") },
                  ...setup.rooms.map((r) => ({ value: r.id, label: r.name })),
                ]}
                className="w-48"
              />
            </div>
          )}
          <div>
            <div className="mb-1 text-xs font-medium text-muted">{t("tt.rules.timeOff")}</div>
            <TimeOffEditor
              shifts={setup.shifts}
              value={timeOff}
              onChange={manage ? setTimeOff : () => {}}
            />
          </div>
        </div>
      )}
    </Card>
  );
}
