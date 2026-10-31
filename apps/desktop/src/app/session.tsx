import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { ApiError, getToken, setToken } from "@/api/client";
import { api } from "@/api/endpoints";
import type { LoginRequest, SchoolSummary, UserSummary } from "@/api/types";
import { DEFAULT_SCHOOL } from "@/lib/school";

export type SessionStatus = "loading" | "anonymous" | "authenticated";

export interface Session {
  status: SessionStatus;
  user: UserSummary | null;
  permissions: ReadonlySet<string>;
  /** The school profile (country, currency, time zone); a Pakistani default until loaded. */
  school: SchoolSummary;
  /** Re-read the school profile after it was changed in Settings. */
  refreshSchool: () => Promise<void>;
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
  initial?: { user: UserSummary; permissions: string[]; school?: SchoolSummary } | "anonymous";
}) {
  const [school, setSchool] = useState<SchoolSummary>(
    initial && initial !== "anonymous" && initial.school ? initial.school : DEFAULT_SCHOOL,
  );
  const refreshSchool = useCallback(async () => {
    try {
      setSchool(await api.currentSchool());
    } catch {
      // Keep whatever we had; every role can read the profile, so this is a network fault.
    }
  }, []);
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
        void refreshSchool();
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
  }, [initial, refreshSchool]);

  const login = useCallback(
    async (credentials: LoginRequest) => {
      const res = await api.login(credentials);
      setToken(res.token);
      const me = await api.me();
      setUser(me.user);
      setPermissions(me.permissions);
      setStatus("authenticated");
      void refreshSchool();
    },
    [refreshSchool],
  );

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
    return {
      status,
      user,
      permissions: set,
      school,
      refreshSchool,
      can: (p) => set.has(p),
      login,
      logout,
    };
  }, [status, user, permissions, school, refreshSchool, login, logout]);

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): Session {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession must be used inside <SessionProvider>");
  return ctx;
}

/** The school profile; the Pakistani default outside a session (tests, setup). */
export function useSchool(): SchoolSummary {
  return useContext(SessionContext)?.school ?? DEFAULT_SCHOOL;
}
