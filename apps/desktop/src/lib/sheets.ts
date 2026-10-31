import type { ImportKind } from "@/api/types";

const WANTED: Record<ImportKind, string[]> = {
  students: ["students", "student", "pupils", "learners", "studentdata"],
  staff: ["staff", "employees", "teachers", "staffdata"],
  opening_balances: ["openingbalances", "balances", "opening", "fees", "arrears"],
};
const SKIP = ["readme", "read", "instructions", "notes", "about", "help", "info", "guide"];

const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");

/**
 * The sheet to preselect for an import kind: one named after the kind ("Students", "Staff"…)
 * first, then the first sheet that is not a read-me or instructions sheet, then the first.
 * Mirrors `pick_sheet` in the backend.
 */
export function pickSheet(kind: ImportKind, sheets: readonly string[]): string | undefined {
  return (
    sheets.find((n) => WANTED[kind].includes(norm(n))) ??
    sheets.find((n) => !SKIP.some((s) => norm(n).startsWith(s))) ??
    sheets[0]
  );
}
