/**
 * The hub's API as the console uses it. The console is served by the hub itself, so the
 * API is on the same origin under `/api/nexus/v1`. The session token lives in
 * `sessionStorage`: closing the tab signs out.
 */
import { ApiError, NetworkError } from "@/api/client";
import type { ApiErrorBody } from "@/api/types";

const PREFIX = "/api/nexus/v1";
const TOKEN_KEY = "madrasa.nexus.session";

export function getHubToken(): string | null {
  try {
    return globalThis.sessionStorage?.getItem(TOKEN_KEY) ?? null;
  } catch {
    return null;
  }
}

export function setHubToken(token: string | null) {
  try {
    if (token) globalThis.sessionStorage?.setItem(TOKEN_KEY, token);
    else globalThis.sessionStorage?.removeItem(TOKEN_KEY);
  } catch {
    // Storage may be unavailable; the session then lasts until the page reloads.
  }
}

/** Called when the hub says the session is no longer valid. */
let onSignedOut: (() => void) | null = null;
export function setSignedOutHandler(handler: (() => void) | null) {
  onSignedOut = handler;
}

async function send(path: string, method: string, body?: unknown): Promise<Response> {
  const headers: Record<string, string> = { Accept: "application/json" };
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const token = getHubToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  let response: Response;
  try {
    response = await fetch(`${PREFIX}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (err) {
    throw new NetworkError(err);
  }
  if (!response.ok) {
    const payload = (await response.json().catch(() => null)) as ApiErrorBody | null;
    const code = payload?.error?.code ?? "unknown";
    if (response.status === 401 && code === "unauthenticated" && token) onSignedOut?.();
    throw new ApiError(response.status, code, payload?.error?.message ?? response.statusText);
  }
  return response;
}

async function call<T>(path: string, method = "GET", body?: unknown): Promise<T> {
  const response = await send(path, method, body);
  if (response.status === 204) return undefined as T;
  const text = await response.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

function qs(params: Record<string, string | number | undefined | null>): string {
  const parts = Object.entries(params)
    .filter(([, v]) => v !== undefined && v !== null && v !== "")
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`);
  return parts.length ? `?${parts.join("&")}` : "";
}

// ---- types (mirroring crates/nexus) -----------------------------------------------------------

export type HubPermission = "nexus.admin" | "nexus.reports" | "nexus.escrow";
export const HUB_PERMISSIONS: HubPermission[] = ["nexus.admin", "nexus.reports", "nexus.escrow"];

export interface HubUser {
  id: string;
  username: string;
  full_name: string;
  email: string | null;
  permissions: string[];
  is_active: boolean;
  locked_until: string | null;
  last_login_at: string | null;
  created_at: string;
}

export interface SetupStatus {
  needs_setup: boolean;
  hub_name: string | null;
}

export interface Me {
  user: HubUser;
  hub_name: string | null;
}

export type SchoolStatus = "joining" | "active" | "left" | "removed" | "purged";

export interface HubSchool {
  id: string;
  school_id: string;
  name: string;
  code: string;
  status: SchoolStatus;
  school_version: string | null;
  edition: string | null;
  joined_at: string | null;
  last_sync_at: string | null;
  left_at: string | null;
  leave_reason: string | null;
  purged_at: string | null;
  /** Sync state of the hub's copy; absent once the data has been purged. */
  sync: { pending: number; parked: number; overwrites: number } | null;
}

export interface Invitation {
  id: string;
  school_name: string;
  note: string | null;
  expires_at: string;
  used_at: string | null;
  revoked_at: string | null;
  created_at: string;
}

export interface CreatedInvitation extends Invitation {
  join_code: string;
  hub_url: string | null;
}

export interface HubOverwrite {
  uid: string | null;
  at: string | null;
  table: string;
  id: string;
  field: string;
  kept: unknown;
  discarded: unknown;
  kept_origin: "school" | "head_office";
  reason: string;
}

export interface HubParked {
  table: string;
  id: string;
  reason: string;
}

export interface ReportEntry {
  code: string;
  title: string;
  about: string;
  takes_period: boolean;
}

export type ColumnKind = "text" | "int" | "money" | "percent" | "date" | "date_time";

export interface HubReport {
  code: string;
  title: string;
  about: string;
  generated_at: string;
  from: string | null;
  to: string | null;
  schools: number;
  columns: { key: string; label: string; kind: ColumnKind }[];
  rows: unknown[][];
  totals: unknown[][];
  notes: string[];
}

export interface ReportParams {
  from?: string;
  to?: string;
  school_id?: string;
  lang: string;
}

export interface EscrowOverview {
  set_up: boolean;
  deposits: {
    school_id: string;
    school_name: string;
    school_code: string;
    key_id: string;
    deposited_at: string;
  }[];
  releases: {
    id: string;
    school_id: string;
    school_name: string;
    school_code: string;
    released_by: string;
    reason: string;
    released_at: string;
    seen_at: string | null;
  }[];
}

export interface ReleasedKey {
  release_id: string;
  school_name: string;
  school_code: string;
  released_at: string;
  released_key: string;
}

export interface CorrectionKind {
  code: string;
  fields: { name: string; required: boolean }[];
}

export interface CorrectionRecord {
  id: string;
  title: string;
  hint: string | null;
  fields: Record<string, string | null>;
}

export interface FoundRecords {
  items: CorrectionRecord[];
  total: number;
  page: number;
  page_size: number;
}

export interface AuditEntry {
  id: string;
  actor_user_id: string | null;
  action: string;
  entity: string;
  entity_id: string | null;
  old_value: unknown;
  new_value: unknown;
  at: string;
}

// ---- endpoints --------------------------------------------------------------------------------

export const hub = {
  setupStatus: () => call<SetupStatus>("/setup/status"),
  setup: (body: {
    setup_token: string;
    hub_name: string;
    admin_username: string;
    admin_full_name: string;
    admin_password: string;
    escrow_passphrase?: string;
  }) => call<{ admin_user_id: string }>("/setup", "POST", body),
  login: (username: string, password: string) =>
    call<{ token: string; user: HubUser }>("/auth/login", "POST", { username, password }),
  logout: () => call<void>("/auth/logout", "POST", {}),
  me: () => call<Me>("/auth/me"),

  schools: () => call<HubSchool[]>("/schools"),
  invitations: () => call<Invitation[]>("/invitations"),
  invite: (school_name: string, note: string) =>
    call<CreatedInvitation>("/invitations", "POST", { school_name, note: note || null }),
  revokeInvitation: (id: string) => call<Invitation>(`/invitations/${id}/revoke`, "POST", {}),
  overwrites: (school: string) => call<HubOverwrite[]>(`/schools/${school}/overwrites`),
  parked: (school: string) => call<HubParked[]>(`/schools/${school}/parked`),
  removeSchool: (school: string, reason: string) =>
    call<HubSchool>(`/schools/${school}/remove`, "POST", { reason }),
  purgeSchool: (school: string, confirm_code: string) =>
    call<HubSchool>(`/schools/${school}/purge`, "POST", { confirm_code }),
  resend: (school: string) => call<{ queued: number }>(`/schools/${school}/resend`, "POST", {}),

  reports: (lang: string) => call<ReportEntry[]>(`/reports${qs({ lang })}`),
  report: (code: string, params: ReportParams) =>
    call<HubReport>(`/reports/${code}${qs({ ...params })}`),
  /** Fetch a report as a file and hand it to the browser as a download. */
  downloadReport: async (code: string, params: ReportParams, format: "csv" | "xlsx") => {
    const response = await send(`/reports/${code}${qs({ ...params, format })}`, "GET");
    const disposition = response.headers.get("content-disposition") ?? "";
    const name = /filename="?([^";]+)"?/.exec(disposition)?.[1] ?? `${code}.${format}`;
    const url = URL.createObjectURL(await response.blob());
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  },

  escrow: () => call<EscrowOverview>("/escrow"),
  escrowSetup: (passphrase: string) => call<void>("/escrow/setup", "POST", { passphrase }),
  escrowPassphrase: (current: string, next: string) =>
    call<void>("/escrow/passphrase", "POST", { current, new: next }),
  releaseKey: (school: string, passphrase: string, reason: string) =>
    call<ReleasedKey>(`/schools/${school}/escrow/release`, "POST", { passphrase, reason }),

  correctionKinds: () => call<CorrectionKind[]>("/corrections/kinds"),
  records: (school: string, kind: string, q: string, page: number) =>
    call<FoundRecords>(`/schools/${school}/records/${kind}${qs({ q, page })}`),
  correct: (school: string, kind: string, record: string, fields: Record<string, string>) =>
    call<{ record: CorrectionRecord; changed: string[] }>(
      `/schools/${school}/records/${kind}/${record}`,
      "PATCH",
      { fields },
    ),

  users: () => call<HubUser[]>("/users"),
  createUser: (body: {
    username: string;
    full_name: string;
    email: string | null;
    password: string;
    permissions: string[];
  }) => call<HubUser>("/users", "POST", body),
  updateUser: (
    id: string,
    body: {
      full_name?: string;
      email?: string | null;
      permissions?: string[];
      is_active?: boolean;
    },
  ) => call<HubUser>(`/users/${id}`, "PATCH", body),
  setPassword: (id: string, password: string) =>
    call<void>(`/users/${id}/password`, "POST", { password }),

  audit: (page: number) => call<AuditEntry[]>(`/audit${qs({ page, page_size: 50 })}`),
};
