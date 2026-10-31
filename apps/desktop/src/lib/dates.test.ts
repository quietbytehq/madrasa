import { describe, expect, it } from "vitest";
import { todayIso } from "./dates";

describe("todayIso", () => {
  it("uses the local calendar date, not the UTC one", () => {
    // 00:30 local on 16 April; in Pakistan (UTC+5) that is still 15 April in UTC.
    const local = new Date(2026, 3, 16, 0, 30);
    expect(todayIso(local)).toBe("2026-04-16");
    expect(todayIso(new Date(2026, 11, 31, 23, 59))).toBe("2026-12-31");
    expect(todayIso(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});
