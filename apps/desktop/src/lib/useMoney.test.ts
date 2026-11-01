import { describe, expect, it } from "vitest";
import { withCurrency } from "./useMoney";

describe("withCurrency", () => {
  it("prefixes the school's symbol and leaves blanks alone", () => {
    expect(withCurrency("Rs", "2500")).toBe("Rs 2,500.00");
    expect(withCurrency("$", 40)).toBe("$ 40.00");
    expect(withCurrency("Rs", null)).toBe("—");
  });
});
