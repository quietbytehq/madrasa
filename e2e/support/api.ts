import { PASSWORD, URLS } from "./env";

/** Straight to the school's API, for setting a scene and for checking what the server holds. */
export async function call<T = unknown>(
  path: string,
  {
    method = "GET",
    body,
    token,
    base = URLS.school,
  }: { method?: string; body?: unknown; token?: string; base?: string } = {},
): Promise<T> {
  const res = await fetch(`${base}/api/v1${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${text.slice(0, 300)}`);
  return (text ? JSON.parse(text) : null) as T;
}

const tokens = new Map<string, string>();

/** A session token for a user of the demo school (cached for the run). */
export async function tokenFor(username: string, password = PASSWORD): Promise<string> {
  const key = `${username}|${password}`;
  let token = tokens.get(key);
  if (!token) {
    token = (
      await call<{ token: string }>("/auth/login", { method: "POST", body: { username, password } })
    ).token;
    tokens.set(key, token);
  }
  return token;
}

export interface DemoUser {
  username: string;
  full_name: string;
  roles: string[];
  is_active: boolean;
}

/** The first active demo login with a role (the demo school's names are generated). */
export async function userWithRole(role: string): Promise<DemoUser> {
  const users = await call<DemoUser[]>("/users", { token: await tokenFor("admin") });
  const found = users.find((u) => u.username !== "admin" && u.roles.includes(role) && u.is_active);
  if (!found) throw new Error(`the demo school has no ${role}`);
  return found;
}
