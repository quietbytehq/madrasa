import { beforeEach, describe, expect, it, vi } from "vitest";
import { memoryKeyStore, setKeyStore } from "@/lib/drafts";
import type { KeyStore } from "@/lib/drafts";
import { discardImport, keepImport, keptImport } from "./importStore";
import type { KeptImport } from "./importStore";

const kept: KeptImport = {
  user: "u1",
  at: "2026-11-20T09:30:00.000Z",
  step: 3,
  kind: "students",
  parsed: {
    file_name: "students.xlsx",
    sheets: ["Sheet1"],
    sheet: "Sheet1",
    headers: ["Student Name", "DOB"],
    rows: [
      ["Ali Khan", "04/03/2015"],
      ["Sara Khan", "not a date"],
    ],
    mapping: { full_name: 0 },
    mapping_saved: false,
  },
  mapping: { full_name: 0, date_of_birth: 1 },
  remember: true,
  policy: "update",
  edited: [
    ["Ali Khan", "04/03/2015"],
    ["Sara Khan", "2016-05-06"],
  ],
};

describe("an unfinished import kept on this PC", () => {
  beforeEach(() => {
    localStorage.clear();
    setKeyStore(memoryKeyStore());
  });

  it("comes back as it was kept, for the person who started it and nobody else", async () => {
    expect(await keptImport("u1")).toBeNull();
    expect(await keepImport(kept)).toBe(true);
    expect(await keptImport("u1")).toEqual(kept);
    expect(await keptImport("u2")).toBeNull();
    // A later state replaces the earlier one: one unfinished import per person.
    await keepImport({ ...kept, step: 2, policy: "skip" });
    expect(await keptImport("u1")).toMatchObject({ step: 2, policy: "skip" });
  });

  it("is sealed: no name, heading or file name can be read from what is stored", async () => {
    await keepImport(kept);
    const stored = Object.keys(localStorage).map((k) => `${k}=${localStorage.getItem(k)}`);
    expect(stored).toHaveLength(1);
    for (const plain of ["Ali Khan", "Sara", "students.xlsx", "Student Name", "2016-05-06"])
      expect(stored[0]).not.toContain(plain);
    expect(stored[0]).not.toContain(btoa("Ali Khan"));
  });

  it("is not offered when it cannot be opened: another key, or changed on the disk", async () => {
    await keepImport(kept);
    const [key] = Object.keys(localStorage);
    const text = localStorage.getItem(key!)!;
    // One character of the sealed text changed: refused whole, not read in part.
    const at = text.length - 8;
    localStorage.setItem(
      key!,
      text.slice(0, at) + (text[at] === "A" ? "B" : "A") + text.slice(at + 1),
    );
    expect(await keptImport("u1")).toBeNull();
    localStorage.setItem(key!, text);
    expect(await keptImport("u1")).toEqual(kept);
    // Another PC's key (or this PC after its key was lost) cannot open it.
    setKeyStore(memoryKeyStore());
    expect(await keptImport("u1")).toBeNull();
  });

  it("is gone once discarded, even when the discard is asked for while a save is under way", async () => {
    const saving = keepImport(kept);
    const discarding = discardImport("u1");
    await Promise.all([saving, discarding]);
    expect(await keptImport("u1")).toBeNull();
    expect(Object.keys(localStorage)).toHaveLength(0);
  });

  it("keeps nothing, and says so, when it cannot be sealed or does not fit", async () => {
    const none: KeyStore = { get: () => Promise.resolve(null) };
    setKeyStore(none);
    expect(await keepImport(kept)).toBe(false);
    expect(Object.keys(localStorage)).toHaveLength(0);

    setKeyStore(memoryKeyStore());
    const full = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("full", "QuotaExceededError");
    });
    expect(await keepImport(kept)).toBe(false);
    full.mockRestore();
    expect(await keptImport("u1")).toBeNull();
  });
});
