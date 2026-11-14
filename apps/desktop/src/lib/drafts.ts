/**
 * Form drafts kept on this PC, encrypted (ADR 0009).
 *
 * Every form autosaves what has been typed as a draft, so a crash, a power cut or closing the
 * app loses nothing: the draft is offered back the next time the same person opens the same
 * form. Drafts are sealed with AES-GCM (the browser's Web Crypto) under a key that is made on
 * first use and kept in this app's IndexedDB as a non-extractable key: the stored drafts
 * cannot be read without it, and the key itself cannot be read out by page code. Where the
 * Web Crypto or IndexedDB is unavailable, nothing is kept rather than keeping it in plain
 * text. Passwords are never part of a draft.
 */

export interface DraftField {
  /** Which control: its name, or its position and type in the form. */
  k: string;
  v: string | boolean | string[];
}

export interface Draft {
  /** The form: user, page and the shape of its fields. */
  key: string;
  user: string;
  path: string;
  /** What the form is called on screen, for the list of drafts. */
  title: string;
  at: string;
  fields: DraftField[];
}

/** Where the encryption key lives. Tests use an in-memory one. */
export interface KeyStore {
  get(): Promise<CryptoKey | null>;
}

const PREFIX = "madrasa.draft.";
const DB = "madrasa-drafts";
const STORE = "keys";
const KEY_ID = "draft-key";

function subtle(): SubtleCrypto | null {
  return globalThis.crypto?.subtle ?? null;
}

/** The key in IndexedDB, made on first use. */
export function indexedDbKeyStore(): KeyStore {
  let cached: Promise<CryptoKey | null> | null = null;
  const open = (): Promise<CryptoKey | null> =>
    new Promise((resolve) => {
      const s = subtle();
      if (!s || typeof indexedDB === "undefined") return resolve(null);
      const req = indexedDB.open(DB, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE);
      req.onerror = () => resolve(null);
      req.onsuccess = () => {
        const db = req.result;
        const read = db.transaction(STORE, "readonly").objectStore(STORE).get(KEY_ID);
        read.onerror = () => resolve(null);
        read.onsuccess = () => {
          if (read.result) return resolve(read.result as CryptoKey);
          s.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"])
            .then((key) => {
              const write = db.transaction(STORE, "readwrite").objectStore(STORE).put(key, KEY_ID);
              write.onsuccess = () => resolve(key);
              write.onerror = () => resolve(null);
            })
            .catch(() => resolve(null));
        };
      };
    });
  return {
    get() {
      cached ??= open();
      return cached;
    },
  };
}

/** A key held in memory only (tests; it is gone with the page). */
export function memoryKeyStore(): KeyStore {
  let key: Promise<CryptoKey | null> | null = null;
  return {
    get() {
      const s = subtle();
      if (!s) return Promise.resolve(null);
      key ??= s.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
      return key;
    },
  };
}

let keys: KeyStore = indexedDbKeyStore();
export function setKeyStore(store: KeyStore) {
  keys = store;
}

function toB64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

function fromB64(s: string): Uint8Array<ArrayBuffer> {
  const bin = atob(s);
  const out = new Uint8Array(new ArrayBuffer(bin.length));
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

async function storageId(key: string): Promise<string | null> {
  const s = subtle();
  if (!s) return null;
  const digest = await s.digest("SHA-256", new TextEncoder().encode(key));
  return (
    PREFIX +
    toB64(new Uint8Array(digest))
      .replace(/[^A-Za-z0-9]/g, "")
      .slice(0, 32)
  );
}

async function seal(value: unknown): Promise<string | null> {
  const s = subtle();
  const key = await keys.get();
  if (!s || !key) return null;
  const iv = globalThis.crypto.getRandomValues(new Uint8Array(12));
  const plain = new TextEncoder().encode(JSON.stringify(value));
  const ct = new Uint8Array(await s.encrypt({ name: "AES-GCM", iv }, key, plain));
  return `${toB64(iv)}.${toB64(ct)}`;
}

async function unseal<T>(text: string): Promise<T | null> {
  const s = subtle();
  const key = await keys.get();
  if (!s || !key) return null;
  const [iv, ct] = text.split(".");
  if (!iv || !ct) return null;
  try {
    const plain = await s.decrypt({ name: "AES-GCM", iv: fromB64(iv) }, key, fromB64(ct));
    return JSON.parse(new TextDecoder().decode(plain)) as T;
  } catch {
    return null;
  }
}

function storage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

/** Keep a draft (replacing the form's previous one). Returns whether it was kept. */
export async function saveDraft(draft: Draft): Promise<boolean> {
  const id = await storageId(draft.key);
  const sealed = await seal(draft);
  const st = storage();
  if (!id || !sealed || !st) return false;
  try {
    st.setItem(id, sealed);
    return true;
  } catch {
    return false;
  }
}

export async function loadDraft(key: string): Promise<Draft | null> {
  const id = await storageId(key);
  const text = id ? storage()?.getItem(id) : null;
  if (!text) return null;
  const draft = await unseal<Draft>(text);
  return draft && draft.key === key ? draft : null;
}

export async function deleteDraft(key: string): Promise<void> {
  const id = await storageId(key);
  if (id) storage()?.removeItem(id);
}

/** This user's drafts, newest first. */
export async function listDrafts(user: string): Promise<Draft[]> {
  const st = storage();
  if (!st) return [];
  const out: Draft[] = [];
  for (let i = 0; i < st.length; i++) {
    const id = st.key(i);
    if (!id?.startsWith(PREFIX)) continue;
    const text = st.getItem(id);
    const d = text ? await unseal<Draft>(text) : null;
    if (d && d.user === user) out.push(d);
  }
  return out.sort((a, b) => b.at.localeCompare(a.at));
}

// ---- reading and filling forms ------------------------------------------------------------

type Control = HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;

const SKIPPED_TYPES = new Set(["password", "file", "hidden", "submit", "button", "reset", "image"]);

function isControl(el: Element): el is Control {
  return (
    el instanceof HTMLInputElement ||
    el instanceof HTMLSelectElement ||
    el instanceof HTMLTextAreaElement
  );
}

/** The controls of a form that drafts keep: never passwords, files or opted-out fields. */
export function draftControls(form: HTMLFormElement): Control[] {
  return Array.from(form.elements).filter((el): el is Control => {
    if (!isControl(el)) return false;
    if (el instanceof HTMLInputElement && SKIPPED_TYPES.has(el.type)) return false;
    const ac = el.getAttribute("autocomplete") ?? "";
    if (/password|one-time-code/.test(ac)) return false;
    return !el.closest("[data-no-draft]");
  });
}

function fieldKey(el: Control, index: number): string {
  if (el instanceof HTMLInputElement && el.type === "radio") {
    return `${el.name || index}=${el.value}`;
  }
  return el.name || `${index}:${el.tagName.toLowerCase()}:${(el as HTMLInputElement).type ?? ""}`;
}

export function readForm(form: HTMLFormElement): DraftField[] {
  return draftControls(form).map((el, i) => {
    const k = fieldKey(el, i);
    if (el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio"))
      return { k, v: el.checked };
    if (el instanceof HTMLSelectElement && el.multiple)
      return { k, v: Array.from(el.selectedOptions).map((o) => o.value) };
    return { k, v: el.value };
  });
}

/** The shape of a form (its fields), which identifies it on its page. */
export function formShape(form: HTMLFormElement): string {
  return draftControls(form)
    .map((el, i) =>
      el instanceof HTMLInputElement && el.type === "radio"
        ? `${el.name || i}=${el.value}`
        : `${el.name || i}:${el.tagName.toLowerCase()}:${(el as HTMLInputElement).type ?? ""}`,
    )
    .join(",");
}

export function sameFields(a: DraftField[], b: DraftField[]): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function setNative(el: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement, value: string) {
  const proto =
    el instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : el instanceof HTMLSelectElement
        ? HTMLSelectElement.prototype
        : HTMLInputElement.prototype;
  // Set through the element's own setter so React notices the change.
  Object.getOwnPropertyDescriptor(proto, "value")?.set?.call(el, value);
  el.dispatchEvent(
    new Event(el instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }),
  );
}

/** Put a draft's values back into the form, as if typed. */
export function fillForm(form: HTMLFormElement, fields: DraftField[]): void {
  const values = new Map(fields.map((f) => [f.k, f.v]));
  draftControls(form).forEach((el, i) => {
    const k = fieldKey(el, i);
    if (!values.has(k)) return;
    const v = values.get(k);
    if (el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio")) {
      if (typeof v === "boolean" && el.checked !== v) el.click();
      return;
    }
    if (el instanceof HTMLSelectElement && el.multiple && Array.isArray(v)) {
      for (const o of Array.from(el.options)) o.selected = v.includes(o.value);
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return;
    }
    if (typeof v === "string" && el.value !== v) setNative(el, v);
  });
}
