import { isStale, marks, monthOf, percent, shiftMonth, thisMonth } from "./format";

describe("format helpers", () => {
  it("moves a month across year ends", () => {
    expect(shiftMonth("2026-11", 1)).toBe("2026-12");
    expect(shiftMonth("2026-12", 1)).toBe("2027-01");
    expect(shiftMonth("2027-01", -1)).toBe("2026-12");
    expect(shiftMonth("2026-04", -4)).toBe("2025-12");
    expect(shiftMonth("nonsense", 1)).toBe("nonsense");
    expect(monthOf("2026-04-01")).toBe("2026-04");
    expect(thisMonth(new Date(2026, 0, 31))).toBe("2026-01");
  });

  it("calls a school quiet after a day without a sync, or when it never synced", () => {
    const now = Date.parse("2026-11-12T12:00:00Z");
    expect(isStale(null, now)).toBe(true);
    expect(isStale("not a date", now)).toBe(true);
    expect(isStale("2026-11-12T11:55:00Z", now)).toBe(false);
    expect(isStale("2026-11-11T12:30:00Z", now)).toBe(false);
    expect(isStale("2026-11-11T11:30:00Z", now)).toBe(true);
  });

  it("writes marks and percentages the way people do", () => {
    expect(marks("45.00")).toBe("45");
    expect(marks("25.50")).toBe("25.5");
    expect(marks(null)).toBe("—");
    expect(percent("90.00")).toBe("90%");
    expect(percent(66.666)).toBe("66.7%");
    expect(percent(null)).toBe("—");
  });
});
