import { beforeEach, describe, expect, it } from "vitest";
import { EMPTY_DRAFT, importDraft } from "./importDraft";

describe("importDraft", () => {
  beforeEach(() => importDraft.clear());
  it("keeps the wizard state between visits and clears on demand", () => {
    expect(importDraft.get()).toEqual(EMPTY_DRAFT);
    importDraft.set({ ...EMPTY_DRAFT, step: 2, kind: "staff", mapping: { full_name: 0 } });
    expect(importDraft.get().step).toBe(2);
    expect(importDraft.get().kind).toBe("staff");
    importDraft.clear();
    expect(importDraft.get()).toEqual(EMPTY_DRAFT);
  });
});
