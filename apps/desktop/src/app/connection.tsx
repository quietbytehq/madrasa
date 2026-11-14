import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { OFFLINE_EVENTS, outboxStatus, ping } from "@/api/client";
import { api } from "@/api/endpoints";
import type { NexusLinkStatus, OutboxLink, OutboxStatus } from "@/api/types";
import { useFeatures } from "@/app/features";
import { useSession } from "@/app/session";

/**
 * How this PC is connected (ADR 0009), for the badge in the top bar.
 *
 * - **LAN client** (the app reaches the school server through this PC's proxy): the proxy's
 *   outbox says whether the server answers and what is waiting.
 * - **Standalone or server** (the database and API are on this PC): the API's health check.
 *
 * And, when the school is joined to a head office and the user may see it, the hub's sync.
 */
export interface Connection {
  link: OutboxLink;
  /** "client": through a LAN proxy with an outbox; "local": the API on this PC. */
  kind: "client" | "local";
  outbox: OutboxStatus | null;
  hub: NexusLinkStatus | null;
  refresh: () => void;
}

const ConnectionContext = createContext<Connection | null>(null);

/** After this long without an answer, "Reconnecting" becomes "Offline" (as in the proxy). */
const GRACE_MS = 15_000;

export function nextLink(
  failingSince: number | null,
  ok: boolean,
  now: number,
): { link: OutboxLink; failingSince: number | null } {
  if (ok) return { link: "connected", failingSince: null };
  const since = failingSince ?? now;
  return { link: now - since >= GRACE_MS ? "offline" : "reconnecting", failingSince: since };
}

export function ConnectionProvider({
  children,
  pollMs = 5000,
}: {
  children: ReactNode;
  pollMs?: number;
}) {
  const { has } = useFeatures();
  const { status: sessionStatus, can } = useSession();
  const [kind, setKind] = useState<"client" | "local">("local");
  const [link, setLink] = useState<OutboxLink>("connected");
  const [outbox, setOutbox] = useState<OutboxStatus | null>(null);
  const [hub, setHub] = useState<NexusLinkStatus | null>(null);
  const failingSince = useRef<number | null>(null);
  const tick = useRef(0);
  const busy = useRef(false);
  const hubVisible =
    has("nexus_link") && sessionStatus === "authenticated" && can("settings.manage");

  const check = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    try {
      const status = await outboxStatus();
      if (status) {
        setKind("client");
        setOutbox(status);
        setLink(status.link);
      } else {
        setKind("local");
        setOutbox(null);
        const ok = await ping();
        const next = nextLink(failingSince.current, ok, Date.now());
        failingSince.current = next.failingSince;
        setLink(next.link);
      }
      // The hub's sync every 20 checks (a minute at the default pace).
      if (hubVisible && tick.current % 20 === 0) {
        api
          .nexusLink()
          .then(setHub)
          .catch(() => {});
      }
      tick.current += 1;
    } finally {
      busy.current = false;
    }
  }, [hubVisible]);

  useEffect(() => {
    void check();
    const timer = setInterval(() => void check(), pollMs);
    const now = () => void check();
    for (const e of Object.values(OFFLINE_EVENTS)) window.addEventListener(e, now);
    return () => {
      clearInterval(timer);
      for (const e of Object.values(OFFLINE_EVENTS)) window.removeEventListener(e, now);
    };
  }, [check, pollMs]);

  useEffect(() => {
    if (!hubVisible) setHub(null);
    tick.current = 0;
  }, [hubVisible]);

  const value = useMemo<Connection>(
    () => ({ link, kind, outbox, hub, refresh: () => void check() }),
    [link, kind, outbox, hub, check],
  );
  return <ConnectionContext.Provider value={value}>{children}</ConnectionContext.Provider>;
}

export function useConnection(): Connection | null {
  return useContext(ConnectionContext);
}
