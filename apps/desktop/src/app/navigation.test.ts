import { describe, expect, it } from "vitest";
import { visibleNavItems } from "./navigation";

const TEACHER = new Set([
  "students.read",
  "attendance.read",
  "attendance.manage",
  "exams.read",
  "exams.manage",
  "teachers.read",
  "academics.read",
  "school.read",
]);

describe("visibleNavItems", () => {
  it("hides Fees, Settings, Backup and Import from a teacher", () => {
    const paths = visibleNavItems(
      () => true,
      (p) => TEACHER.has(p),
    ).map((i) => i.path);
    expect(paths).toEqual(
      expect.arrayContaining(["/", "/students", "/attendance", "/exams", "/teachers"]),
    );
    expect(paths).not.toContain("/fees");
    expect(paths).not.toContain("/settings");
    expect(paths).not.toContain("/backup");
    expect(paths).not.toContain("/import");
  });
  it("shows everything the edition enables to an administrator", () => {
    const all = visibleNavItems(
      () => true,
      () => true,
    ).map((i) => i.path);
    expect(all).toEqual(expect.arrayContaining(["/fees", "/settings", "/backup", "/import"]));
  });
});
