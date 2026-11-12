/** Small pure helpers for what the screens show. */

/** `2026-11` moved by a number of months. */
export function shiftMonth(month: string, by: number): string {
  const [y, m] = month.split("-").map(Number);
  if (!y || !m) return month;
  const index = y * 12 + (m - 1) + by;
  return `${Math.floor(index / 12)}-${String((index % 12) + 1).padStart(2, "0")}`;
}

/** The month (`YYYY-MM`) a date or timestamp falls in. */
export function monthOf(date: string): string {
  return date.slice(0, 7);
}

/** This month on the phone's calendar, as `YYYY-MM`. */
export function thisMonth(now: Date = new Date()): string {
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * Has the school gone quiet? True when it never synced or last synced more than a day
 * ago: what the app shows may then be behind the school's own records.
 */
export function isStale(syncedAt: string | null, now: number = Date.now()): boolean {
  if (!syncedAt) return true;
  const at = Date.parse(syncedAt);
  return Number.isNaN(at) || now - at > DAY_MS;
}

/** Marks as people write them: `45.00` → `45`, `25.50` → `25.5`. */
export function marks(value: string | number | null | undefined): string {
  if (value === null || value === undefined || value === "") return "—";
  const n = Number(value);
  if (!Number.isFinite(n)) return String(value);
  return String(Math.round(n * 100) / 100);
}

/** A percentage with at most one decimal: `90.00` → `90%`. */
export function percent(value: string | number | null | undefined): string {
  if (value === null || value === undefined || value === "") return "—";
  const n = Number(value);
  if (!Number.isFinite(n)) return "—";
  return `${Math.round(n * 10) / 10}%`;
}
