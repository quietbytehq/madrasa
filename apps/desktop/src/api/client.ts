import type { ApiErrorBody } from "./types";

const TOKEN_KEY = "madrasa.session";

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
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
    throw new NetworkError(err);
  }

  if (response.status === 204) return undefined as T;

  let payload: unknown = null;
  const text = await response.text();
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch {
      payload = null;
    }
  }

  if (!response.ok) {
    const body = payload as ApiErrorBody | null;
    throw new ApiError(
      response.status,
      body?.error?.code ?? "unknown",
      body?.error?.message ?? response.statusText,
    );
  }
  return payload as T;
}
