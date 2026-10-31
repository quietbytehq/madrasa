import { describe, expect, it } from "vitest";
import { firstYearStart, monthOptions } from "./months";

describe("months", () => {
  it("lists twelve months in the UI language", () => {
    const en = monthOptions("en");
    expect(en).toHaveLength(12);
    expect(en[3]).toEqual({ value: "4", label: "April" });
    expect(monthOptions("ur")[0]?.value).toBe("1");
  });
  it("finds the year the current academic year started in", () => {
    expect(firstYearStart(new Date(2026, 9, 31), 4)).toBe(2026);
    expect(firstYearStart(new Date(2026, 3, 1), 4)).toBe(2026);
    expect(firstYearStart(new Date(2026, 1, 10), 4)).toBe(2025);
    expect(firstYearStart(new Date(2026, 7, 20), 9)).toBe(2025);
  });
});
