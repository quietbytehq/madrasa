import type {
  LessonView,
  Named,
  Room,
  TimetableCard,
  TimetableShift,
  TimetableView,
} from "@/api/types";

export type GridKind = "section" | "teacher" | "room";

/** One placed session as drawn in a cell. */
export interface GridCell {
  card: TimetableCard;
  lesson: LessonView;
  /** Title line: the subject. */
  title: string;
  /** Second line: who or where, depending on the grid kind. */
  detail: string;
  room: string;
  /** True on the second and later rows of a double or triple period. */
  continued: boolean;
}

export interface GridRow {
  period: number;
  time: string;
  /** One entry per working day (same order as `GridShift.days`). */
  cells: (GridCell | null)[];
}

export interface GridShift {
  shift: TimetableShift;
  /** Working day indexes, 0 = Monday. */
  days: number[];
  rows: GridRow[];
  /** Periods after which a break falls (for a divider). */
  breaksAfter: number[];
}

/** Shifts a section, teacher or room takes part in. */
export function shiftsFor(view: TimetableView, kind: GridKind, id: string): TimetableShift[] {
  const enabled = view.shifts.filter((s) => s.enabled);
  if (kind === "section") {
    // A section attends one shift: the one its cards are in, else the first enabled shift.
    const used = new Set(
      view.cards
        .filter((c) => view.lessons.find((l) => l.id === c.lesson_id)?.section_ids.includes(id))
        .map((c) => c.shift_id),
    );
    const own = enabled.filter((s) => used.has(s.id));
    return own.length > 0 ? own : enabled.slice(0, 1);
  }
  return enabled;
}

function belongs(lesson: LessonView, card: TimetableCard, kind: GridKind, id: string): boolean {
  switch (kind) {
    case "section":
      return lesson.section_ids.includes(id);
    case "teacher":
      return lesson.teacher_ids.includes(id);
    case "room":
      return card.room_id === id;
  }
}

function roomName(rooms: Room[], id: string | null): string {
  return id ? (rooms.find((r) => r.id === id)?.name ?? "") : "";
}

/**
 * Lay out the week of one section, teacher or room: one block per shift, a row per period
 * and a column per working day. Double periods fill every row they span.
 */
export function buildGrid(view: TimetableView, kind: GridKind, id: string): GridShift[] {
  return shiftsFor(view, kind, id).map((shift) => {
    const days = shift.days
      .map((d, i) => (d.working && d.periods > 0 ? i : -1))
      .filter((i) => i >= 0);
    const periods = Math.max(0, ...shift.days.map((d) => (d.working ? d.periods : 0)));
    const rows: GridRow[] = [];
    for (let p = 1; p <= periods; p += 1) {
      const time = shift.period_times[p - 1];
      rows.push({
        period: p,
        time: time ? `${time.start}–${time.end}` : "",
        cells: days.map((day) => {
          if ((shift.days[day]?.periods ?? 0) < p) return null;
          const card = view.cards.find(
            (c) =>
              c.shift_id === shift.id &&
              c.day === day &&
              c.period <= p &&
              p < c.period + c.length &&
              belongs(
                view.lessons.find((l) => l.id === c.lesson_id)!,
                c,
                kind,
                id,
              ),
          );
          if (!card) return null;
          const lesson = view.lessons.find((l) => l.id === card.lesson_id)!;
          const room = roomName(view.rooms, card.room_id);
          const detail =
            kind === "section"
              ? lesson.teachers.join(", ")
              : kind === "teacher"
                ? lesson.sections.join(", ")
                : `${lesson.sections.join(", ")} · ${lesson.teachers.join(", ")}`;
          return {
            card,
            lesson,
            title: lesson.subject,
            detail,
            room,
            continued: p > card.period,
          };
        }),
      });
    }
    return {
      shift,
      days,
      rows,
      breaksAfter: shift.breaks.map((b) => b.after_period),
    };
  });
}

/** Choices for the "show" selector of a kind. */
export function optionsFor(view: TimetableView, kind: GridKind): Named[] {
  switch (kind) {
    case "section":
      return view.sections;
    case "teacher":
      return view.teachers;
    case "room":
      return view.rooms.map((r) => ({ id: r.id, name: r.name }));
  }
}

/** Sessions of a lesson in placement order, for "session 2 of 5". */
export function sessionCount(view: TimetableView, lessonId: string): number {
  return view.lessons.find((l) => l.id === lessonId)?.cards_per_week ?? 0;
}
