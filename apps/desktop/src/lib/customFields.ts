/**
 * A record's `custom` fields hold what the school added itself (a blood group, a bus stop)
 * and also notes Madrasa keeps for its own use: the mark on demo records, where a row came
 * from in an import. Only the school's own fields are shown and edited; the rest stays on the
 * record untouched.
 */
const INTERNAL =
  /^(_|demo$|import[_.-]|imported[_.-]?|import$|source$|batch[_.-]?|row[_.-]?|legacy[_.-]|sync[_.-]|hlc$)/i;

export function isInternalCustomField(key: string, value: unknown): boolean {
  if (INTERNAL.test(key.trim())) return true;
  // Something only a program wrote: a nested object or a list.
  return typeof value === "object" && value !== null;
}

/** The fields to show, as text. */
export function visibleCustomFields(custom: Record<string, unknown> | null | undefined) {
  return Object.entries(custom ?? {})
    .filter(([key, value]) => !isInternalCustomField(key, value))
    .map(([key, value]) => ({ key, value: value === null ? "" : String(value) }));
}

/** The fields kept out of sight, to put back when the record is saved. */
export function internalCustomFields(
  custom: Record<string, unknown> | null | undefined,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(custom ?? {}).filter(([key, value]) => isInternalCustomField(key, value)),
  );
}
