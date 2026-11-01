import type {
  DuplicatePolicy,
  ImportKind,
  ImportPreview,
  ImportResult,
  ParsedFile,
} from "@/api/types";

/** Everything the import wizard holds between steps, kept while the user visits other pages. */
export interface ImportDraft {
  step: number;
  kind: ImportKind;
  parsed: ParsedFile | null;
  mapping: Record<string, number>;
  remember: boolean;
  policy: DuplicatePolicy;
  preview: ImportPreview | null;
  edited: string[][] | null;
  result: ImportResult | null;
}

export const EMPTY_DRAFT: ImportDraft = {
  step: 0,
  kind: "students",
  parsed: null,
  mapping: {},
  remember: true,
  policy: "skip",
  preview: null,
  edited: null,
  result: null,
};

let draft: ImportDraft = EMPTY_DRAFT;

/** The draft lives for the app session (not across restarts: a file would be stale anyway). */
export const importDraft = {
  get: (): ImportDraft => draft,
  set: (next: ImportDraft) => {
    draft = next;
  },
  clear: () => {
    draft = EMPTY_DRAFT;
  },
};
