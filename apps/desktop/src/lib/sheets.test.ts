import { describe, expect, it } from "vitest";
import { pickSheet } from "./sheets";

describe("pickSheet", () => {
  it("prefers a sheet named after the import kind over a read-me", () => {
    const wb = ["Read Me", "Students", "Staff"];
    expect(pickSheet("students", wb)).toBe("Students");
    expect(pickSheet("staff", wb)).toBe("Staff");
    expect(pickSheet("opening_balances", ["Instructions", "Opening Balances"])).toBe(
      "Opening Balances",
    );
  });
  it("skips read-me style sheets, then falls back to the first", () => {
    expect(pickSheet("students", ["README", "Notes", "Data"])).toBe("Data");
    expect(pickSheet("students", ["Read me"])).toBe("Read me");
    expect(pickSheet("students", [])).toBeUndefined();
  });
});
