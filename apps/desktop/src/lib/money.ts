/**
 * Money helpers. Amounts travel as decimal strings; arithmetic happens on the server. The UI
 * only formats and validates, never computes fees.
 */

/** Format a decimal string as PKR with two places, e.g. "2,500.00". */
export function formatMoney(value: string | number | null | undefined, locale = "en-PK"): string {
  if (value === null || value === undefined || value === "") return "—";
  const n = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(n)) return String(value);
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(n);
}

/** Validate a user-typed amount; returns a normalised "1234.50" string or null. */
export function parseMoney(input: string): string | null {
  const cleaned = input.replace(/[,\s]/g, "");
  if (!/^\d+(\.\d{0,2})?$/.test(cleaned)) return null;
  const [whole, frac = ""] = cleaned.split(".");
  return `${whole}.${(frac + "00").slice(0, 2)}`;
}

export function isZero(value: string | null | undefined): boolean {
  return !value || Number(value) === 0;
}

/** First day of the month as YYYY-MM-01. */
export function monthStart(d = new Date()): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;
}

/** Human month label from a YYYY-MM-DD date string. */
export function monthLabel(date: string, locale = "en-PK"): string {
  const [y, m] = date.split("-").map(Number);
  if (!y || !m) return date;
  return new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" }).format(
    new Date(y, m - 1, 1),
  );
}
