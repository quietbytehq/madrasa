import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { getHubUrl, getToken, hubApi, setHubUrl, setSignedOutHandler, setToken } from "./api";
import type { ParentSession } from "./api";

const NAME_KEY = "madrasa.parent.name";
const PHONE_KEY = "madrasa.parent.phone";

function stored(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function store(key: string, value: string | null) {
  try {
    if (value) globalThis.localStorage?.setItem(key, value);
    else globalThis.localStorage?.removeItem(key);
  } catch {
    // ignore
  }
}

export interface Session {
  signedIn: boolean;
  /** The parent's name, as the school has it. */
  name: string | null;
  /** The phone number the parent signs in with. */
  phone: string | null;
  /** The hub the app is set up for, if any (kept after signing out). */
  hubUrl: string | null;
  /** True once after the hub ended the session; the sign-in screen says so. */
  expired: boolean;
  signIn: (hubUrl: string, session: ParentSession) => void;
  signOut: () => Promise<void>;
}

const SessionContext = createContext<Session | null>(null);

/**
 * Who is using the app. The session token, the hub's address and the parent's name are
 * kept in the app's own storage so the app opens signed in; nothing else is stored on the
 * phone, and no child's record is kept between visits.
 */
export function SessionProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState(() => ({
    signedIn: !!getToken() && !!getHubUrl(),
    name: stored(NAME_KEY),
    phone: stored(PHONE_KEY),
    hubUrl: getHubUrl(),
    expired: false,
  }));

  const clear = useCallback((expired: boolean) => {
    setToken(null);
    store(NAME_KEY, null);
    setState((s) => ({ ...s, signedIn: false, name: null, expired }));
  }, []);

  const signIn = useCallback((hubUrl: string, session: ParentSession) => {
    setHubUrl(hubUrl);
    setToken(session.token);
    store(NAME_KEY, session.parent.full_name);
    store(PHONE_KEY, session.parent.phone);
    setState({
      signedIn: true,
      name: session.parent.full_name,
      phone: session.parent.phone,
      hubUrl,
      expired: false,
    });
  }, []);

  const signOut = useCallback(async () => {
    // Tell the hub when it can be reached; either way this phone forgets the session.
    await hubApi.logout().catch(() => null);
    clear(false);
  }, [clear]);

  useEffect(() => {
    setSignedOutHandler(() => clear(true));
    return () => setSignedOutHandler(null);
  }, [clear]);

  const value = useMemo<Session>(() => ({ ...state, signIn, signOut }), [state, signIn, signOut]);
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): Session {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession must be used inside <SessionProvider>");
  return ctx;
}
