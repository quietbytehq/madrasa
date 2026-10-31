/** Today as YYYY-MM-DD in the PC's local time zone (the school's), for date inputs and
 * defaults. Never use toISOString() for this: it gives the UTC date, which is yesterday for
 * the first hours of every morning east of Greenwich. */
export function todayIso(now: Date = new Date()): string {
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}
