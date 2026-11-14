import { useEffect, useRef, useState } from "react";
import { OFFLINE_EVENTS } from "@/api/client";
import type { OutboxLink } from "@/api/types";
import { nextLink } from "@/app/connection";
import { REACHED, pingHub } from "@parent/api";

/**
 * Is the school network's hub reachable from this phone (ADR 0009)? Shown on every screen.
 * Follows every call the app makes, the phone's own network state, and a check of the hub
 * every half minute (every few seconds while it is not answering).
 */
export function useHubLink(): OutboxLink {
  const [link, setLink] = useState<OutboxLink>(() =>
    typeof navigator !== "undefined" && navigator.onLine === false ? "offline" : "connected",
  );
  const failingSince = useRef<number | null>(null);
  useEffect(() => {
    let cancelled = false;
    const report = (ok: boolean) => {
      if (cancelled) return;
      const next = nextLink(failingSince.current, ok, Date.now());
      failingSince.current = next.failingSince;
      // No network at all on the phone: offline at once.
      setLink(!ok && navigator.onLine === false ? "offline" : next.link);
    };
    const check = () => void pingHub().then(report);
    const reached = () => report(true);
    const failed = () => report(false);
    window.addEventListener(REACHED, reached);
    window.addEventListener(OFFLINE_EVENTS.unreachable, failed);
    window.addEventListener("online", check);
    window.addEventListener("offline", failed);
    let timer: ReturnType<typeof setTimeout>;
    const loop = () => {
      check();
      timer = setTimeout(loop, failingSince.current === null ? 30_000 : 5_000);
    };
    loop();
    return () => {
      cancelled = true;
      clearTimeout(timer);
      window.removeEventListener(REACHED, reached);
      window.removeEventListener(OFFLINE_EVENTS.unreachable, failed);
      window.removeEventListener("online", check);
      window.removeEventListener("offline", failed);
    };
  }, []);
  return link;
}
