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
import type { NexusLinkStatus, OutboxLink, OutboxStatus } from "@/api/types";
import { publishHubStatus, refreshHubStatus, useHubStatus } from "@/app/hubStatus";
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

/**
 * How often the connection is looked at. On a LAN client the question goes to this PC's own
 * proxy (no network involved), so it is asked every second: together with the proxy's
 * two-second heartbeat a pulled cable shows as "Reconnecting…" within about five seconds, and
 * "Offline" fifteen seconds after the server's last answer. With the database on this PC
 * there is no network to lose and every five seconds is enough.
 */
const CLIENT_MS = 1000;
const LOCAL_MS = 5000;
/** How long between two looks at the sync with head office (the school's background syncs). */
const HUB_MS = 15_000;

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
  pollMs,
}: {
  children: ReactNode;
  /** A fixed pace (tests); by default the pace follows the kind of connection. */
  pollMs?: number;
}) {
  const { has } = useFeatures();
  const { status: sessionStatus, can } = useSession();
  const [kind, setKind] = useState<"client" | "local">("local");
  const [link, setLink] = useState<OutboxLink>("connected");
  const [outbox, setOutbox] = useState<OutboxStatus | null>(null);
  // One source for the badge and the Settings card (app/hubStatus.ts).
  const sharedHub = useHubStatus();
  const failingSince = useRef<number | null>(null);
  const pace = useRef(LOCAL_MS);
  // The last status shown, to leave the screen alone while nothing changed.
  const shown = useRef("");
  const hubAskedAt = useRef(0);
  const busy = useRef(false);
  const hubVisible =
    has("nexus_link") && sessionStatus === "authenticated" && can("settings.manage");

  const check = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    try {
      const status = await outboxStatus();
      if (status) {
        pace.current = CLIENT_MS;
        setKind("client");
        // While connected the time of the last answer moves with every heartbeat; that
        // alone is no reason to draw every screen that shows a voucher again.
        const text = JSON.stringify({
          ...status,
          last_contact: status.link === "connected" ? null : status.last_contact,
        });
        if (text !== shown.current) {
          shown.current = text;
          setOutbox(status);
        }
        setLink(status.link);
      } else {
        pace.current = LOCAL_MS;
        shown.current = "";
        setKind("local");
        setOutbox(null);
        const ok = await ping();
        const next = nextLink(failingSince.current, ok, Date.now());
        failingSince.current = next.failingSince;
        setLink(next.link);
      }
      // The hub's sync every fifteen seconds, which catches the school's own background
      // syncs; a sync from the screen is published the moment it answers.
      if (hubVisible && Date.now() - hubAskedAt.current >= HUB_MS) {
        hubAskedAt.current = Date.now();
        void refreshHubStatus();
      }
    } finally {
      busy.current = false;
    }
  }, [hubVisible]);

  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const loop = async () => {
      await check();
      if (!stopped) timer = setTimeout(() => void loop(), pollMs ?? pace.current);
    };
    void loop();
    const now = () => void check();
    for (const e of Object.values(OFFLINE_EVENTS)) window.addEventListener(e, now);
    return () => {
      stopped = true;
      clearTimeout(timer);
      for (const e of Object.values(OFFLINE_EVENTS)) window.removeEventListener(e, now);
    };
  }, [check, pollMs]);

  useEffect(() => {
    if (!hubVisible) publishHubStatus(null);
    hubAskedAt.current = 0;
  }, [hubVisible]);

  const hub = hubVisible ? sharedHub : null;
  const value = useMemo<Connection>(
    () => ({ link, kind, outbox, hub, refresh: () => void check() }),
    [link, kind, outbox, hub, check],
  );
  return <ConnectionContext.Provider value={value}>{children}</ConnectionContext.Provider>;
}

export function useConnection(): Connection | null {
  return useContext(ConnectionContext);
}
