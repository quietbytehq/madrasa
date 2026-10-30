import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { ApiError, getToken, setToken } from "@/api/client";
import { api } from "@/api/endpoints";
import type { LoginRequest, UserSummary } from "@/api/types";

export type SessionStatus = "loading" | "anonymous" | "authenticated";

export interface Session {
  status: SessionStatus;
  user: UserSummary | null;
  permissions: ReadonlySet<string>;
  can: (permission: string) => boolean;
  login: (credentials: LoginRequest) => Promise<void>;
  logout: () => Promise<void>;
}

const SessionContext = createContext<Session | null>(null);

export function SessionProvider({
  children,
  initial,
}: {
  children: ReactNode;
  /** Preset session (tests). */
  initial?: { user: UserSummary; permissions: string[] } | "anonymous";
}) {
  const [status, setStatus] = useState<SessionStatus>(() => {
    if (initial === "anonymous") return "anonymous";
    if (initial) return "authenticated";
    return getToken() ? "loading" : "anonymous";
  });
  const [user, setUser] = useState<UserSummary | null>(
    initial && initial !== "anonymous" ? initial.user : null,
  );
  const [permissions, setPermissions] = useState<string[]>(
    initial && initial !== "anonymous" ? initial.permissions : [],
  );

  useEffect(() => {
    if (initial || !getToken()) return;
    let cancelled = false;
    api
      .me()
      .then((me) => {
        if (cancelled) return;
        setUser(me.user);
        setPermissions(me.permissions);
        setStatus("authenticated");
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        if (err instanceof ApiError && err.status === 401) setToken(null);
        setUser(null);
        setPermissions([]);
        setStatus("anonymous");
      });
    return () => {
      cancelled = true;
    };
  }, [initial]);

  const login = useCallback(async (credentials: LoginRequest) => {
    const res = await api.login(credentials);
    setToken(res.token);
    const me = await api.me();
    setUser(me.user);
    setPermissions(me.permissions);
    setStatus("authenticated");
  }, []);

  const logout = useCallback(async () => {
    try {
      await api.logout();
    } catch {
      // Token may already be invalid; clear locally regardless.
    }
    setToken(null);
    setUser(null);
    setPermissions([]);
    setStatus("anonymous");
  }, []);

  const value = useMemo<Session>(() => {
    const set = new Set(permissions);
    return { status, user, permissions: set, can: (p) => set.has(p), login, logout };
  }, [status, user, permissions, login, logout]);

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): Session {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession must be used inside <SessionProvider>");
  return ctx;
}
