/**
 * An import that was started and not yet confirmed, kept on this PC so that closing the app,
 * a crash or a power cut does not lose it.
 *
 * What is kept is what the person did: the file as it was read, the columns they matched,
 * the choices they made and the cells they corrected. The check against the school's records
 * is not kept: it is run again when the import is continued, because the records may have
 * changed in between.
 *
 * It is sealed exactly like a form draft (`@/lib/drafts`: AES-GCM under a key that cannot be
 * read out of this app), one per person, and removed when the import is confirmed or
 * discarded. A sheet of 5,000 rows is larger than a browser lets a page keep in
 * `localStorage`, so the sealed text goes into IndexedDB; where there is no IndexedDB it
 * goes into `localStorage` if it fits. Where it cannot be sealed or does not fit, nothing is
 * kept, and the page says so.
 */
import type { DuplicatePolicy, ImportKind, ParsedFile } from "@/api/types";
import { sealValue, unsealValue } from "@/lib/drafts";

export interface KeptImport {
  /** Whose import it is (user id): another person at this PC is not offered it. */
  user: string;
  /** When the file was read: "the import you started on …". */
  at: string;
  /** How far it had got: 2 = matching columns, 3 = checked. */
  step: 2 | 3;
  kind: ImportKind;
  parsed: ParsedFile;
  mapping: Record<string, number>;
  remember: boolean;
  policy: DuplicatePolicy;
  /** The rows with the person's corrections, when any were made. */
  edited: string[][] | null;
}

const DB = "madrasa-import";
const STORE = "kept";
const PREFIX = "madrasa.import.";

function open(): Promise<IDBDatabase | null> {
  return new Promise((resolve) => {
    if (typeof indexedDB === "undefined") return resolve(null);
    try {
      const req = indexedDB.open(DB, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE);
      req.onerror = () => resolve(null);
      req.onsuccess = () => resolve(req.result);
    } catch {
      resolve(null);
    }
  });
}

function done<T>(req: IDBRequest<T>): Promise<T | null> {
  return new Promise((resolve) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => resolve(null);
  });
}

function local(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

async function put(id: string, text: string): Promise<boolean> {
  const db = await open();
  if (db) {
    try {
      const tx = db.transaction(STORE, "readwrite");
      const settled = new Promise<boolean>((resolve) => {
        tx.oncomplete = () => resolve(true);
        tx.onerror = () => resolve(false);
        tx.onabort = () => resolve(false);
      });
      tx.objectStore(STORE).put(text, id);
      return await settled;
    } catch {
      return false;
    } finally {
      db.close();
    }
  }
  try {
    local()?.setItem(PREFIX + id, text);
    return local() !== null;
  } catch {
    // Too large for localStorage: nothing is kept rather than a part of it.
    local()?.removeItem(PREFIX + id);
    return false;
  }
}

async function get(id: string): Promise<string | null> {
  const db = await open();
  if (db) {
    try {
      const text = await done(db.transaction(STORE, "readonly").objectStore(STORE).get(id));
      return typeof text === "string" ? text : null;
    } catch {
      return null;
    } finally {
      db.close();
    }
  }
  return local()?.getItem(PREFIX + id) ?? null;
}

async function remove(id: string): Promise<void> {
  const db = await open();
  if (db) {
    try {
      await done(db.transaction(STORE, "readwrite").objectStore(STORE).delete(id));
    } catch {
      // Nothing to remove.
    } finally {
      db.close();
    }
  }
  local()?.removeItem(PREFIX + id);
}

/** One thing at a time, in the order asked: a discard never overtakes the save before it. */
let last: Promise<unknown> = Promise.resolve();
function inOrder<T>(work: () => Promise<T>): Promise<T> {
  const next = last.then(work, work);
  last = next.catch(() => undefined);
  return next;
}

/** Keep (or replace) this person's unfinished import. Says whether it was kept. */
export function keepImport(kept: KeptImport): Promise<boolean> {
  return inOrder(async () => {
    const sealed = await sealValue(kept);
    if (!sealed) return false;
    return put(kept.user, sealed);
  });
}

/** This person's unfinished import, if one was kept on this PC and can still be opened. */
export function keptImport(user: string): Promise<KeptImport | null> {
  return inOrder(async () => {
    const text = await get(user);
    if (!text) return null;
    const kept = await unsealValue<KeptImport>(text);
    // Sealed for another person, by another key, or not an import at all: not offered.
    if (!kept || kept.user !== user || !kept.parsed || !Array.isArray(kept.parsed.rows))
      return null;
    return kept;
  });
}

export function discardImport(user: string): Promise<void> {
  return inOrder(() => remove(user));
}
