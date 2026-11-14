import type { ApiErrorBody, OutboxStatus, ProvisionalReceipt, QueuedInfo } from "./types";
import { isLocale, translate } from "@/i18n";

const TOKEN_KEY = "madrasa.session";

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
    /** Structured detail some refusals carry (for example the login that already exists). */
    public readonly details: unknown = null,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/**
 * The school server was out of reach and the change was kept on this PC (LAN client mode,
 * ADR 0009): it will be sent on its own when the server answers. Not a failure, but the
 * caller did not get the server's answer either, so it is thrown like one; its message says
 * what happened, in the user's language.
 */
export class QueuedError extends ApiError {
  constructor(public readonly queued: QueuedInfo) {
    super(202, "queued", queuedMessage(queued));
    this.name = "QueuedError";
  }
}

function currentLocale() {
  const lang = typeof document === "undefined" ? "en" : document.documentElement.lang;
  return isLocale(lang) ? lang : "en";
}

function queuedMessage(q: QueuedInfo): string {
  return q.provisional
    ? translate(currentLocale(), "offline.queued.receipt", {
        number: q.provisional.provisional_no,
      })
    : translate(currentLocale(), "offline.queued", { count: q.waiting });
}

let lastQueued = "";
/** The message of the last change kept offline (toasts show it as information, not an error). */
export function lastQueuedMessage(): string {
  return lastQueued;
}

/** Events the connection badge and the form drafts listen to. */
export const OFFLINE_EVENTS = {
  /** A change was kept on this PC (`detail`: QueuedInfo). */
  queued: "madrasa:queued",
  /** A change reached the server. */
  saved: "madrasa:saved",
  /** A screen was shown from the copy kept on this PC. */
  cached: "madrasa:cached",
  /** The server (or this PC's proxy) did not answer. */
  unreachable: "madrasa:unreachable",
} as const;

function emit(name: string, detail?: unknown) {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(name, { detail }));
}

/** Thrown when the API cannot be reached at all. */
export class NetworkError extends Error {
  constructor(cause: unknown) {
    super("network");
    this.name = "NetworkError";
    this.cause = cause;
  }
}

let baseUrl: string | null = null;

/**
 * Resolve the API base URL once. Inside Tauri the shell knows the port of the in-process
 * API and exposes it through the `api_base_url` command; in the browser (vite dev) we fall
 * back to VITE_API_BASE_URL or the default dev port.
 */
export async function getBaseUrl(): Promise<string> {
  if (baseUrl) return baseUrl;
  const fromEnv = import.meta.env.VITE_API_BASE_URL as string | undefined;
  if ("__TAURI_INTERNALS__" in window) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      baseUrl = await invoke<string>("api_base_url");
      return baseUrl;
    } catch (err) {
      console.warn("api_base_url command failed, falling back to env", err);
    }
  }
  baseUrl = (fromEnv ?? "http://127.0.0.1:4777").replace(/\/$/, "");
  return baseUrl;
}

/** Test/dev helper to pin the base URL. */
export function setBaseUrl(url: string | null) {
  baseUrl = url;
}

export function getToken(): string | null {
  try {
    return globalThis.localStorage?.getItem(TOKEN_KEY) ?? null;
  } catch {
    return null;
  }
}

export function setToken(token: string | null) {
  try {
    if (token) globalThis.localStorage?.setItem(TOKEN_KEY, token);
    else globalThis.localStorage?.removeItem(TOKEN_KEY);
  } catch {
    // ignore
  }
}

export interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  auth?: boolean;
  signal?: AbortSignal;
}

export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const base = await getBaseUrl();
  const headers: Record<string, string> = { Accept: "application/json" };
  if (options.body !== undefined) headers["Content-Type"] = "application/json";
  const token = options.auth === false ? null : getToken();
  if (token) headers.Authorization = `Bearer ${token}`;

  let response: Response;
  try {
    response = await fetch(`${base}/api/v1${path}`, {
      method: options.method ?? "GET",
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: options.signal ?? null,
    });
  } catch (err) {
    emit(OFFLINE_EVENTS.unreachable);
    throw new NetworkError(err);
  }

  const method = options.method ?? "GET";
  const changes = method !== "GET";
  if (response.headers.get("x-madrasa-offline") === "cached") emit(OFFLINE_EVENTS.cached);
  if (response.status === 204) {
    if (changes) emit(OFFLINE_EVENTS.saved);
    return undefined as T;
  }

  let payload: unknown = null;
  const text = await response.text();
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch {
      payload = null;
    }
  }

  if (response.status === 202 && payload && typeof payload === "object" && "queued" in payload) {
    const queued = (payload as { queued: QueuedInfo }).queued;
    const err = new QueuedError(queued);
    lastQueued = err.message;
    emit(OFFLINE_EVENTS.queued, queued);
    throw err;
  }

  if (!response.ok) {
    const body = payload as ApiErrorBody | null;
    if (body?.error?.code === "server_unreachable") emit(OFFLINE_EVENTS.unreachable);
    throw new ApiError(
      response.status,
      body?.error?.code ?? "unknown",
      body?.error?.message ?? response.statusText,
      body?.details ?? null,
    );
  }
  if (changes) emit(OFFLINE_EVENTS.saved);
  return payload as T;
}

// ---- the LAN client's outbox (only in client mode; ADR 0009) ------------------------------

async function local<T>(path: string, init?: RequestInit): Promise<T | null> {
  const base = await getBaseUrl();
  try {
    const res = await fetch(`${base}/__madrasa/outbox${path}`, init);
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

/** The outbox's state, or `null` when this PC does not run through a LAN proxy. */
export function outboxStatus(): Promise<OutboxStatus | null> {
  return local<OutboxStatus>("/status").then((s) => (s && "link" in s ? s : null));
}

export function outboxRetry(): Promise<unknown> {
  return local("/retry", { method: "POST" });
}

export function outboxDismiss(
  kind: "failed" | "overwritten" | "notices",
  id?: string,
): Promise<OutboxStatus | null> {
  return local<OutboxStatus>("/dismiss", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ kind, id }),
  });
}

export function provisionalReceipt(number: string): Promise<ProvisionalReceipt | null> {
  return local<ProvisionalReceipt>(`/receipts/${encodeURIComponent(number)}`);
}

/** Is the API answering? (Standalone and server mode: the database and API on this PC.) */
export async function ping(): Promise<boolean> {
  const base = await getBaseUrl();
  try {
    const res = await fetch(`${base}/api/v1/health`, { cache: "no-store" });
    return res.ok;
  } catch {
    return false;
  }
}

/** Fetch a file from the API (with the session token) and hand it to the browser as a download. */
export async function download(path: string): Promise<void> {
  const base = await getBaseUrl();
  const headers: Record<string, string> = {};
  const token = getToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  let res: Response;
  try {
    res = await fetch(`${base}/api/v1${path}`, { headers });
  } catch (err) {
    throw new NetworkError(err);
  }
  if (!res.ok) {
    const body = (await res.json().catch(() => null)) as ApiErrorBody | null;
    throw new ApiError(
      res.status,
      body?.error?.code ?? "error",
      body?.error?.message ?? res.statusText,
    );
  }
  const disposition = res.headers.get("content-disposition") ?? "";
  const match = /filename="?([^";]+)"?/.exec(disposition);
  const name = match?.[1] ?? "download";
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
