import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { ParentAppStatus } from "@/api/types";

export { PARENT_APP_PERMISSIONS } from "./parentAppPermissions";

const CHANGED = "madrasa:parent-app-changed";
const REFRESH_MS = 2 * 60 * 1000;

/** Tell the sidebar that the link to the school network or the unread count changed. */
export function announceParentAppChange() {
  globalThis.dispatchEvent?.(new Event(CHANGED));
}

/**
 * Whether the school is joined to a school network, and the caller's unread messages from
 * parents. `null` until known, when not asked for, or when the service cannot say; the
 * Parent app entry stays hidden then. Asked again every two minutes and after a change.
 */
export function useParentAppStatus(enabled: boolean): ParentAppStatus | null {
  const [status, setStatus] = useState<ParentAppStatus | null>(null);
  useEffect(() => {
    if (!enabled) {
      setStatus(null);
      return;
    }
    let cancelled = false;
    const load = () => {
      api.parentAppStatus().then(
        (s) => !cancelled && setStatus(s),
        () => !cancelled && setStatus(null),
      );
    };
    load();
    const timer = setInterval(load, REFRESH_MS);
    globalThis.addEventListener?.(CHANGED, load);
    return () => {
      cancelled = true;
      clearInterval(timer);
      globalThis.removeEventListener?.(CHANGED, load);
    };
  }, [enabled]);
  return status;
}
