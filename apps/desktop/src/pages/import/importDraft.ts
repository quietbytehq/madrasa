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
  /** When the file was read; the import is kept on this PC under this date (importStore). */
  startedAt: string | null;
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
  startedAt: null,
};

let draft: ImportDraft = EMPTY_DRAFT;

/**
 * The wizard's state while the app is open, so that a visit to another page loses nothing.
 * What must outlive the app itself (the file, the matched columns, the corrections) is kept
 * sealed on this PC by `importStore`.
 */
export const importDraft = {
  get: (): ImportDraft => draft,
  set: (next: ImportDraft) => {
    draft = next;
  },
  clear: () => {
    draft = EMPTY_DRAFT;
  },
};
