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
  it("shows the Parent app only to a joined school and to people with something to do there", () => {
    const paths = (can: (p: string) => boolean, joined: boolean) =>
      visibleNavItems(() => true, can, joined).map((i) => i.path);
    const teacher = (p: string) => TEACHER.has(p) || p === "messages.reply";
    // Not joined to a school network: nobody sees it, whatever they may do.
    expect(paths(() => true, false)).not.toContain("/parent-app");
    expect(paths(teacher, false)).not.toContain("/parent-app");
    // Joined: a class teacher (messages), an administrator; not someone without any of it.
    expect(paths(teacher, true)).toContain("/parent-app");
    expect(paths((p) => p === "notices.manage", true)).toContain("/parent-app");
    expect(paths((p) => TEACHER.has(p), true)).not.toContain("/parent-app");
    // An edition without the school network link has no Parent app.
    expect(
      visibleNavItems(
        (f) => f !== "nexus_link",
        () => true,
        true,
      ).map((i) => i.path),
    ).not.toContain("/parent-app");
  });
});
