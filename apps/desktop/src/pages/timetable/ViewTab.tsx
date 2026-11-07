import { useEffect, useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import type { SlotOption, SwapOption, TimetableSetup, TimetableView } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { TimetableGrid } from "./TimetableGrid";
import { Hint, StatusBadge, dayLabel } from "./common";
import { optionsFor, sessionCount } from "./grid";
import type { GridCell, GridKind } from "./grid";

interface Props {
  setup: TimetableSetup;
  manage: boolean;
  /** The timetable to show; empty = the published one. */
  timetableId: string;
  onPickTimetable: (id: string) => void;
}

/** The timetable by section, teacher or room, with manual moves on drafts. */
export function ViewTab({ setup, manage, timetableId, onPickTimetable }: Props) {
  const { t, locale } = useI18n();
  const toast = useToast();
  const [kind, setKind] = useState<GridKind>("section");
  const [id, setId] = useState("");
  const view = useAsync(
    () => (timetableId ? api.timetable(timetableId) : api.currentTimetable(setup.academic_year.id)),
    [timetableId, setup.academic_year.id],
  );
  const data: TimetableView | null = view.data;
  const choices = useMemo(() => (data ? optionsFor(data, kind) : []), [data, kind]);
  useEffect(() => {
    if (!id && choices.length > 0) setId(choices[0]?.id ?? "");
    if (id && !choices.some((c) => c.id === id)) setId(choices[0]?.id ?? "");
  }, [choices, id]);
  const [editing, setEditing] = useState<GridCell | null>(null);
  const editable = manage && !!data?.timetable;
  const print = (lang: string) => {
    if (!id) return;
    void api
      .printTimetable(kind, id, lang, data?.timetable?.id)
      .catch((err) =>
        toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) }),
      );
  };
  return (
    <div className="flex flex-col gap-4">
      <Card>
        <div className="flex flex-wrap items-end gap-3">
          <Select
            label={t("tt.view.which")}
            value={timetableId}
            onChange={(e) => onPickTimetable(e.target.value)}
            options={[
              { value: "", label: t("tt.view.current") },
              ...setup.timetables.map((x) => ({
                value: x.id,
                label: `${x.name}${x.status === "published" ? ` · ${t("tt.timetable.published")}` : ""}`,
              })),
            ]}
            className="w-56"
          />
          <Select
            label={t("tt.view.kind")}
            value={kind}
            onChange={(e) => {
              setKind(e.target.value as GridKind);
              setId("");
            }}
            options={[
              { value: "section", label: t("tt.view.section") },
              { value: "teacher", label: t("tt.view.teacher") },
              { value: "room", label: t("tt.view.room") },
            ]}
            className="w-36"
          />
          <Select
            label={t(`tt.view.${kind}`)}
            value={id}
            onChange={(e) => setId(e.target.value)}
            options={choices.map((c) => ({ value: c.id, label: c.name }))}
            className="w-56"
          />
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="secondary"
              disabled={!id || !data?.timetable}
              onClick={() => print(locale)}
            >
              {t("tt.view.print")}
            </Button>
            {locale !== "ur" && (
              <Button
                size="sm"
                variant="ghost"
                disabled={!id || !data?.timetable}
                onClick={() => print("ur")}
              >
                {t("tt.view.printUr")}
              </Button>
            )}
          </div>
          {data?.timetable && (
            <div className="flex items-center gap-2 pb-2 text-sm">
              <span className="font-medium">{data.timetable.name}</span>
              <StatusBadge status={data.timetable.status} />
              {data.job && <Badge tone="info">{t("tt.generate.running")}</Badge>}
            </div>
          )}
        </div>
      </Card>
      {view.loading && !data ? (
        <div className="flex justify-center py-10">
          <Spinner size="lg" />
        </div>
      ) : !data?.timetable ? (
        <Card>
          <EmptyState title={t("tt.view.none")} />
        </Card>
      ) : (
        <Card>
          {data.problems.length > 0 && (
            <div
              className="mb-3 rounded border border-warning bg-warning-bg p-2 text-xs"
              data-testid="problems"
            >
              <div className="font-medium">{t("tt.view.problems")}</div>
              <ul className="mt-1 list-inside list-disc">
                {data.problems.slice(0, 10).map((p, i) => (
                  <li key={i}>{p}</li>
                ))}
              </ul>
            </div>
          )}
          {editable && <Hint>{t("tt.view.editHint")}</Hint>}
          {!id ? (
            <EmptyState title={t("tt.view.pick")} />
          ) : (
            <TimetableGrid
              view={data}
              kind={kind}
              id={id}
              onCellClick={editable ? setEditing : undefined}
            />
          )}
        </Card>
      )}
      {editing && data?.timetable && (
        <MoveDialog
          timetableId={data.timetable.id}
          view={data}
          cell={editing}
          onClose={() => setEditing(null)}
          onChanged={(next) => {
            view.setData(next);
            setEditing(null);
          }}
        />
      )}
    </div>
  );
}

function MoveDialog({
  timetableId,
  view,
  cell,
  onClose,
  onChanged,
}: {
  timetableId: string;
  view: TimetableView;
  cell: GridCell;
  onClose: () => void;
  onChanged: (view: TimetableView) => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const { card, lesson } = cell;
  const slots = useAsync(
    () => api.slotOptions(timetableId, card.lesson_id, card.number),
    [timetableId, card.id],
  );
  const swaps = useAsync(
    () => api.swapOptions(timetableId, card.lesson_id, card.number),
    [timetableId, card.id],
  );
  const [busy, setBusy] = useState(false);
  const [room, setRoom] = useState<Record<string, string>>({});
  const act = async (f: () => Promise<TimetableView | unknown>, reloadAfter = false) => {
    setBusy(true);
    try {
      const r = await f();
      if (reloadAfter) onChanged(await api.timetable(timetableId));
      else onChanged(r as TimetableView);
      toast.push({ kind: "success", title: t("tt.edit.moved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const shiftName = (id: string) => view.shifts.find((s) => s.id === id)?.name ?? "";
  const many = view.shifts.filter((s) => s.enabled).length > 1;
  const slotKey = (s: SlotOption) => `${s.shift}-${s.day}-${s.period}`;
  const tone = (s: SlotOption["status"]) =>
    s === "ok" ? "success" : s === "soft" ? "warning" : "danger";
  const label = (s: SlotOption["status"]) =>
    s === "ok" ? t("tt.edit.ok") : s === "soft" ? t("tt.edit.soft") : t("tt.edit.blocked");
  const options: SlotOption[] = (slots.data ?? []).filter((s) => s.status !== "blocked");
  const blocked = (slots.data ?? []).filter((s) => s.status === "blocked").length;
  const swapOptions: SwapOption[] = (swaps.data ?? []).filter((s) => s.ok);
  return (
    <Dialog
      open
      onClose={onClose}
      title={`${t("tt.edit.title")}: ${lesson.subject} · ${lesson.sections.join(", ")}`}
      description={`${t("tt.edit.session", { n: card.number, total: sessionCount(view, card.lesson_id) })} · ${dayLabel(t, card.day)} ${card.period}${card.locked ? ` · ${t("tt.view.locked")}` : ""}`}
      size="lg"
      footer={
        <div className="flex w-full items-center justify-between">
          <Button
            variant="secondary"
            size="sm"
            loading={busy}
            onClick={() =>
              void act(
                () =>
                  api.lockCard(timetableId, {
                    lesson_id: card.lesson_id,
                    number: card.number,
                    locked: !card.locked,
                  }),
                true,
              )
            }
          >
            {card.locked ? t("tt.edit.unlock") : t("tt.edit.lock")}
          </Button>
          <Button variant="secondary" onClick={onClose}>
            {t("action.close")}
          </Button>
        </div>
      }
    >
      {slots.loading ? (
        <Spinner />
      ) : (
        <div className="flex flex-col gap-3">
          <div>
            <div className="mb-1 text-xs font-medium text-muted">
              {t("tt.edit.slots")}
              {blocked > 0 && (
                <span className="ms-2">
                  ({blocked} {t("tt.edit.blocked").toLowerCase()})
                </span>
              )}
            </div>
            <ul
              className="flex max-h-72 flex-col gap-1 overflow-y-auto text-sm"
              data-testid="slot-options"
            >
              {options.map((s) => {
                const key = slotKey(s);
                const rooms = s.rooms.filter((r) => r.free);
                return (
                  <li
                    key={key}
                    className="flex flex-wrap items-center gap-2 rounded border border-border px-2 py-1"
                  >
                    <Badge tone={tone(s.status)}>{label(s.status)}</Badge>
                    <span className="font-medium">
                      {many && `${shiftName(s.shift)} · `}
                      {dayLabel(t, s.day)} {s.period}
                    </span>
                    {s.reasons.length > 0 && (
                      <span className="text-xs text-muted">{s.reasons.join("; ")}</span>
                    )}
                    {s.rooms.length > 0 && (
                      <Select
                        aria-label={t("tt.edit.room")}
                        value={room[key] ?? rooms[0]?.id ?? ""}
                        onChange={(e) => setRoom({ ...room, [key]: e.target.value })}
                        options={rooms.map((r) => ({ value: r.id, label: r.name }))}
                        className="w-36"
                      />
                    )}
                    <Button
                      size="sm"
                      disabled={busy || card.locked}
                      className="ms-auto"
                      onClick={() =>
                        void act(() =>
                          api.moveCard(timetableId, {
                            lesson_id: card.lesson_id,
                            number: card.number,
                            day: s.day,
                            period: s.period,
                            room_id: s.rooms.length ? (room[key] ?? rooms[0]?.id ?? null) : null,
                          }),
                        )
                      }
                    >
                      {t("tt.edit.move")}
                    </Button>
                  </li>
                );
              })}
            </ul>
          </div>
          <div>
            <div className="mb-1 text-xs font-medium text-muted">{t("tt.edit.swap")}</div>
            {swapOptions.length === 0 ? (
              <p className="text-xs text-muted">{t("tt.edit.swapNone")}</p>
            ) : (
              <ul className="flex max-h-40 flex-col gap-1 overflow-y-auto text-sm">
                {swapOptions.slice(0, 30).map((s) => {
                  const other = view.lessons.find((l) => l.id === s.lessonId);
                  const placed = view.cards.find(
                    (c) => c.lesson_id === s.lessonId && c.number === s.number,
                  );
                  return (
                    <li
                      key={`${s.lessonId}-${s.number}`}
                      className="flex items-center gap-2 rounded border border-border px-2 py-1"
                    >
                      <span>
                        {other?.subject} · {other?.sections.join(", ")}
                        {placed && ` · ${dayLabel(t, placed.day)} ${placed.period}`}
                      </span>
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy || card.locked}
                        className="ms-auto"
                        onClick={() =>
                          void act(() =>
                            api.swapCards(timetableId, {
                              lesson_id: card.lesson_id,
                              number: card.number,
                              with_lesson_id: s.lessonId,
                              with_number: s.number,
                            }),
                          )
                        }
                      >
                        {t("tt.edit.swap")}
                      </Button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}
