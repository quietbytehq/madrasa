/** Month names 1–12 in the UI language, for start-month selects. */
export function monthOptions(locale: string): Array<{ value: string; label: string }> {
  const fmt = new Intl.DateTimeFormat(locale === "ur" ? "ur-PK" : "en", { month: "long" });
  return Array.from({ length: 12 }, (_, i) => ({
    value: String(i + 1),
    label: fmt.format(new Date(2026, i, 1)),
  }));
}

/**
 * The calendar year the current academic year started in: this year once the start month has
 * arrived, else last year. Mirrors `first_year_start` in the backend.
 */
export function firstYearStart(today: Date, startMonth: number): number {
  return today.getMonth() + 1 >= startMonth ? today.getFullYear() : today.getFullYear() - 1;
}
