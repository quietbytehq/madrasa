import { describe, expect, it } from "vitest";
import type { LessonView, TimetableShift, TimetableView } from "@/api/types";
import { buildGrid, optionsFor, shiftsFor } from "./grid";

const shift: TimetableShift = {
  id: "sh",
  school_id: "s",
  academic_year_id: "y",
  name: "Morning",
  enabled: true,
  days: [
    { working: true, periods: 3 },
    { working: true, periods: 3 },
    { working: false, periods: 0 },
    { working: true, periods: 2 },
    { working: false, periods: 0 },
    { working: false, periods: 0 },
    { working: false, periods: 0 },
  ],
  period_times: [
    { start: "08:00", end: "08:40" },
    { start: "08:40", end: "09:20" },
    { start: "09:40", end: "10:20" },
  ],
  breaks: [{ after_period: 2, name: "Break", cannot_cross: true }],
  position: 0,
  hlc: "",
};

const lesson = (id: string, subject: string, teacher: string, section: string): LessonView => ({
  id,
  school_id: "s",
  academic_year_id: "y",
  subject_id: `subj-${subject}`,
  teacher_ids: [teacher],
  section_ids: [section],
  room_ids: [],
  periods_per_card: 1,
  cards_per_week: 2,
  hlc: "",
  subject,
  subject_short: subject.slice(0, 2),
  teachers: [teacher === "t1" ? "Sana Malik" : "Bilal Ahmed"],
  sections: [section === "c1" ? "9 A" : "9 B"],
  rooms: [],
});

const view: TimetableView = {
  timetable: null,
  academic_year: {} as TimetableView["academic_year"],
  shifts: [shift, { ...shift, id: "off", enabled: false }],
  lessons: [
    lesson("l1", "Maths", "t1", "c1"),
    lesson("l2", "Urdu", "t2", "c1"),
    lesson("l3", "Maths", "t1", "c2"),
  ],
  cards: [
    {
      id: "a",
      timetable_id: "t",
      lesson_id: "l1",
      number: 1,
      shift_id: "sh",
      day: 0,
      period: 1,
      length: 2,
      room_id: "r1",
      locked: false,
    },
    {
      id: "b",
      timetable_id: "t",
      lesson_id: "l2",
      number: 1,
      shift_id: "sh",
      day: 1,
      period: 3,
      length: 1,
      room_id: null,
      locked: true,
    },
    {
      id: "c",
      timetable_id: "t",
      lesson_id: "l3",
      number: 1,
      shift_id: "sh",
      day: 3,
      period: 2,
      length: 1,
      room_id: "r1",
      locked: false,
    },
  ],
  rooms: [
    {
      id: "r1",
      school_id: "s",
      name: "Room 1",
      short_name: "R1",
      building: "",
      capacity: null,
      hlc: "",
    },
  ],
  sections: [
    { id: "c1", name: "9 A" },
    { id: "c2", name: "9 B" },
  ],
  teachers: [
    { id: "t1", name: "Sana Malik" },
    { id: "t2", name: "Bilal Ahmed" },
  ],
  problems: [],
  job: null,
  covers: [],
  today: null,
};

describe("timetable grid", () => {
  it("lays out a section's week with double periods on every row they span", () => {
    const g = buildGrid(view, "section", "c1")[0]!;
    expect(g.days).toEqual([0, 1, 3]);
    expect(g.rows.map((r) => r.period)).toEqual([1, 2, 3]);
    expect(g.rows[0]!.time).toBe("08:00–08:40");
    expect(g.breaksAfter).toEqual([2]);
    // Monday: Maths spans periods 1 and 2.
    expect(g.rows[0]!.cells[0]?.title).toBe("Maths");
    expect(g.rows[0]!.cells[0]?.detail).toBe("Sana Malik");
    expect(g.rows[0]!.cells[0]?.room).toBe("Room 1");
    expect(g.rows[0]!.cells[0]?.continued).toBe(false);
    expect(g.rows[1]!.cells[0]?.continued).toBe(true);
    expect(g.rows[2]!.cells[0]).toBeNull();
    // Tuesday period 3: Urdu; Thursday only has two periods, so period 3 is off.
    expect(g.rows[2]!.cells[1]?.title).toBe("Urdu");
    expect(g.rows[2]!.cells[1]?.card.locked).toBe(true);
    expect(g.rows[2]!.cells[2]).toBeNull();
    // 9 B's Maths does not appear in 9 A's grid.
    expect(g.rows[1]!.cells[2]).toBeNull();
  });

  it("shows a teacher's sections and a room's occupants", () => {
    const t = buildGrid(view, "teacher", "t1")[0]!;
    expect(t.rows[0]!.cells[0]?.detail).toBe("9 A");
    expect(t.rows[1]!.cells[2]?.detail).toBe("9 B");
    expect(t.rows[2]!.cells[1]).toBeNull();
    const r = buildGrid(view, "room", "r1")[0]!;
    expect(r.rows[0]!.cells[0]?.detail).toBe("9 A · Sana Malik");
    expect(r.rows[2]!.cells[1]).toBeNull();
  });

  it("ignores disabled shifts and offers the right choices", () => {
    expect(shiftsFor(view, "teacher", "t1").map((s) => s.id)).toEqual(["sh"]);
    expect(shiftsFor(view, "section", "c1").map((s) => s.id)).toEqual(["sh"]);
    expect(optionsFor(view, "room")).toEqual([{ id: "r1", name: "Room 1" }]);
    expect(optionsFor(view, "teacher").length).toBe(2);
  });
});
