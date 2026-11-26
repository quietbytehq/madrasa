import { describe, expect, it } from "vitest";
import { pct } from "./common";

describe("pct", () => {
  it("shows one decimal place, as the printed report card does", () => {
    expect(pct("85")).toBe("85.0%");
    expect(pct("91.26")).toBe("91.3%");
    expect(pct("91.24")).toBe("91.2%");
    expect(pct(100)).toBe("100.0%");
    expect(pct("0")).toBe("0.0%");
  });

  it("rounds a half to the even digit, like the server's PDF", () => {
    expect(pct("78.25")).toBe("78.2%");
    expect(pct("78.35")).toBe("78.4%");
    expect(pct("78.15")).toBe("78.2%");
    expect(pct("99.95")).toBe("100.0%");
  });

  it("shows a dash when there is no figure", () => {
    expect(pct(null)).toBe("—");
    expect(pct(undefined)).toBe("—");
    expect(pct("")).toBe("—");
    expect(pct("n/a")).toBe("—");
  });
});
