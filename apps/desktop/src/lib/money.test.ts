import { formatMoney, isZero, monthLabel, monthStart, parseMoney } from "./money";

describe("money helpers", () => {
  it("formats decimals with two places and grouping", () => {
    expect(formatMoney("2500")).toBe("2,500.00");
    expect(formatMoney("3333.33")).toBe("3,333.33");
    expect(formatMoney("0")).toBe("0.00");
    expect(formatMoney(null)).toBe("—");
    expect(formatMoney("-470.00")).toBe("-470.00");
  });
  it("parses typed amounts strictly", () => {
    expect(parseMoney("1,000")).toBe("1000.00");
    expect(parseMoney("12.5")).toBe("12.50");
    expect(parseMoney("12.345")).toBeNull();
    expect(parseMoney("abc")).toBeNull();
    expect(parseMoney("-5")).toBeNull();
  });
  it("month helpers", () => {
    expect(monthStart(new Date(2026, 3, 17))).toBe("2026-04-01");
    expect(monthLabel("2026-04-01")).toBe("April 2026");
    expect(isZero("0")).toBe(true);
    expect(isZero("0.00")).toBe(true);
    expect(isZero("1.00")).toBe(false);
  });
});
