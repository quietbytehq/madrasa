/**
 * The Nexus hub's API as the parent app uses it (docs/spec.md §17.2, §17.3). The app talks
 * to the hub only, never to a school. The hub's address is typed in once (it is printed on
 * the access slip) and kept, with the session token, in the app's own storage.
 */
import type { ReportCard } from "@/api/types";
import { OFFLINE_EVENTS } from "@/api/client";

const PREFIX = "/api/nexus/v1";
/** The hub answered a call. */
export const REACHED = "madrasa:reached";
const HUB_KEY = "madrasa.parent.hub";
const TOKEN_KEY = "madrasa.parent.session";

function read(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function write(key: string, value: string | null) {
  try {
    if (value) globalThis.localStorage?.setItem(key, value);
    else globalThis.localStorage?.removeItem(key);
  } catch {
    // Storage may be unavailable; the value then lasts until the app is closed.
  }
}

export const getHubUrl = () => read(HUB_KEY);
export const setHubUrl = (url: string | null) => write(HUB_KEY, url);
export const getToken = () => read(TOKEN_KEY);
export const setToken = (token: string | null) => write(TOKEN_KEY, token);

/** Hosts a plain `http://` address is accepted for: this device and the Android emulator. */
const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]", "10.0.2.2"]);

/**
 * The hub address as typed, tidied: `https://` is added when no scheme is given, a path
 * and trailing slashes are dropped. `null` when it is not an address, or when it is plain
 * `http://` to anything but this device: sign-in details and children's records travel
 * encrypted or not at all.
 */
export function normalizeHubUrl(input: string): string | null {
  const typed = input.trim();
  if (!typed || /\s/.test(typed)) return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(typed) ? typed : `https://${typed}`;
  let url: URL;
  try {
    url = new URL(withScheme);
  } catch {
    return null;
  }
  if (
    url.username ||
    url.password ||
    (!url.hostname.includes(".") && !LOCAL_HOSTS.has(url.hostname))
  ) {
    return null;
  }
  if (url.protocol === "http:" ? !LOCAL_HOSTS.has(url.hostname) : url.protocol !== "https:") {
    return null;
  }
  return url.origin;
}

/** The hub answered, and refused. `message` is the hub's own plain sentence. */
export class HubError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "HubError";
  }
}

/** The hub could not be reached at all: no connection, a wrong address, the hub is down. */
export class OfflineError extends Error {
  constructor(cause?: unknown) {
    super("offline");
    this.name = "OfflineError";
    this.cause = cause;
  }
}

/** Called when the hub says the session is no longer valid. */
let onSignedOut: (() => void) | null = null;
export function setSignedOutHandler(handler: (() => void) | null) {
  onSignedOut = handler;
}

interface CallOptions {
  method?: "GET" | "POST";
  body?: unknown;
  /** Use this hub address instead of the stored one (before signing in). */
  hub?: string;
  /** Send the session token. On by default. */
  auth?: boolean;
}

/** Tell the connection badge (and form drafts) how the last call went (ADR 0009). */
function emit(name: string) {
  if (typeof window !== "undefined") window.dispatchEvent(new CustomEvent(name));
}

/** Does the hub answer? For the connection badge. */
export async function pingHub(): Promise<boolean> {
  const hub = getHubUrl();
  if (!hub) return true;
  try {
    const res = await fetch(`${hub}${PREFIX}/health`, { credentials: "omit", cache: "no-store" });
    return res.ok;
  } catch {
    return false;
  }
}

async function call<T>(path: string, options: CallOptions = {}): Promise<T> {
  const hub = options.hub ?? getHubUrl();
  if (!hub) throw new OfflineError("no hub address");
  const headers: Record<string, string> = { Accept: "application/json" };
  if (options.body !== undefined) headers["Content-Type"] = "application/json";
  const token = options.auth === false ? null : getToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  let response: Response;
  try {
    response = await fetch(`${hub}${PREFIX}${path}`, {
      method: options.method ?? "GET",
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      // Never cookies: the token in the header is the only thing that signs a request.
      credentials: "omit",
    });
  } catch (err) {
    emit(OFFLINE_EVENTS.unreachable);
    throw new OfflineError(err);
  }
  emit(REACHED);
  const text = await response.text().catch(() => "");
  let payload: unknown;
  try {
    payload = text ? JSON.parse(text) : null;
  } catch {
    // Not the hub's JSON: something else answered at this address.
    if (!response.ok) throw new HubError(response.status, "unknown", "");
    throw new OfflineError("not a hub");
  }
  if (!response.ok) {
    const error = (payload as { error?: { code?: string; message?: string } } | null)?.error;
    const code = error?.code ?? "unknown";
    if (response.status === 401 && code === "unauthenticated" && token) onSignedOut?.();
    throw new HubError(response.status, code, error?.message ?? "");
  }
  if ((options.method ?? "GET") !== "GET") emit(OFFLINE_EVENTS.saved);
  return payload as T;
}

// ---- what the hub sends (mirrors crates/nexus/src/routes/parents.rs and parent_data.rs) ------

export interface ParentAccount {
  id: string;
  phone: string;
  full_name: string;
  last_login_at: string | null;
}

export interface ParentSession {
  token: string;
  expires_at: string;
  parent: ParentAccount;
}

/** What an access code is for, before it is used. */
export interface CodeCheck {
  guardian_name: string;
  school_name: string;
  children: string[];
  /** This guardian already has an account: the last digits of its phone number. */
  existing_phone_hint: string | null;
}

export interface Me {
  parent: ParentAccount;
  hub_name: string | null;
  links: number;
}

export interface Child {
  /** With `student_id`, names the child in every call. */
  school_ref: string;
  school_name: string;
  synced_at: string | null;
  student_id: string;
  full_name: string;
  student_number: string;
  status: string;
  relationship: "father" | "mother" | "guardian" | "other";
  class_name: string | null;
  section_name: string | null;
  roll_no: number | null;
  unread: number;
}

export interface Counts {
  present: number;
  absent: number;
  late: number;
  leave: number;
  total: number;
  percent: number | null;
}

/** Every answer about a child says when the school last synced. */
export interface Synced {
  synced_at: string | null;
}

export interface Overview extends Synced, Omit<Child, "synced_at" | "unread"> {
  class_teacher: string | null;
  academic_year: string | null;
  attendance: Counts | null;
  currency: string;
  balance: string;
  open_vouchers: number;
  unread: number;
  notices: number;
}

export type DayStatus = "present" | "absent" | "late" | "leave";

export interface AttendanceMonth extends Synced {
  /** `YYYY-MM`. */
  month: string;
  days: { date: string; status: DayStatus }[];
  summary: Counts;
  year: { name: string; start_date: string; end_date: string; summary: Counts } | null;
}

export interface Results extends Synced {
  /** From approved marks only; `null` when the school has no active year. */
  card: ReportCard | null;
}

export interface Voucher {
  id: string;
  voucher_no: string;
  billing_month: string;
  issue_date: string;
  due_date: string;
  lines: { description: string; amount: string }[];
  gross: string;
  sibling_discount: string;
  concession_discount: string;
  net_monthly: string;
  arrears: string;
  total_due: string;
  late_fee: string;
  net_paid: string;
  balance: string;
  status: "unpaid" | "partial" | "paid" | "carried";
}

export interface Receipt {
  id: string;
  receipt_no: string;
  voucher_no: string | null;
  paid_on: string;
  amount: string;
  method: string;
  reversal: boolean;
}

export interface FeeStatus extends Synced {
  currency: string;
  balance: string;
  vouchers: Voucher[];
  receipts: Receipt[];
}

export interface Notice {
  id: string;
  title: string;
  body: string;
  published_at: string | null;
  expires_on: string | null;
  whole_school: boolean;
}

export interface Notices extends Synced {
  notices: Notice[];
}

export interface Message {
  id: string;
  sender: "guardian" | "staff";
  sender_name: string;
  body: string;
  sent_at: string;
}

export interface Conversation extends Synced {
  class_teacher: string | null;
  class_name: string | null;
  section_name: string | null;
  can_send: boolean;
  messages: Message[];
  unread: number;
}

/** A child in a call: the school's reference at the hub and the student. */
export interface ChildRef {
  school: string;
  student: string;
}

const child = ({ school, student }: ChildRef, tail = "") =>
  `/parent/children/${encodeURIComponent(school)}/${encodeURIComponent(student)}${tail}`;

export const hubApi = {
  /** Whose code is this? Asked of a hub the app is not signed in to yet. */
  checkCode: (hub: string, code: string) =>
    call<CodeCheck>("/parent/activate/check", { method: "POST", body: { code }, hub, auth: false }),
  activate: (hub: string, code: string, phone: string, password: string) =>
    call<ParentSession>("/parent/activate", {
      method: "POST",
      body: { code, phone, password },
      hub,
      auth: false,
    }),
  login: (hub: string, phone: string, password: string) =>
    call<ParentSession>("/parent/login", {
      method: "POST",
      body: { phone, password },
      hub,
      auth: false,
    }),
  logout: () => call<null>("/parent/logout", { method: "POST", body: {} }),
  me: () => call<Me>("/parent/me"),
  children: () => call<{ children: Child[] }>("/parent/children"),
  overview: (c: ChildRef) => call<Overview>(child(c)),
  attendance: (c: ChildRef, month?: string) =>
    call<AttendanceMonth>(child(c, month ? `/attendance?month=${month}` : "/attendance")),
  results: (c: ChildRef) => call<Results>(child(c, "/results")),
  fees: (c: ChildRef) => call<FeeStatus>(child(c, "/fees")),
  notices: (c: ChildRef) => call<Notices>(child(c, "/notices")),
  messages: (c: ChildRef) => call<Conversation>(child(c, "/messages")),
  sendMessage: (c: ChildRef, body: string) =>
    call<Message>(child(c, "/messages"), { method: "POST", body: { body } }),
  markRead: (c: ChildRef) =>
    call<{ unread: number }>(child(c, "/messages/read"), { method: "POST", body: {} }),
};
