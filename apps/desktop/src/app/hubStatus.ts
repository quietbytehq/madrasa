import { useSyncExternalStore } from "react";
import { api } from "@/api/endpoints";
import type { NexusLinkStatus } from "@/api/types";

/**
 * The school's sync with head office, kept in one place.
 *
 * The badge in the top bar and the card under Settings → School network both read this, so
 * they can never say different things: when a sync finishes (pressed in the card, or the
 * school's own every few minutes), both change in the same moment. Before v0.5.1 each asked
 * the server by itself, and the badge said "in step" for a minute after the card had stopped
 * saying so.
 */
let current: NexusLinkStatus | null = null;
const listeners = new Set<() => void>();
let asking: Promise<NexusLinkStatus | null> | null = null;

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The newest answer from the server (from a sync, joining, leaving, or a look). */
export function publishHubStatus(next: NexusLinkStatus | null): void {
  current = next;
  for (const l of listeners) l();
}

/** Ask the server how the sync stands and tell everyone who shows it. */
export function refreshHubStatus(): Promise<NexusLinkStatus | null> {
  asking ??= api
    .nexusLink()
    .then(
      (next) => {
        publishHubStatus(next);
        return next;
      },
      () => current,
    )
    .finally(() => {
      asking = null;
    });
  return asking;
}

export function hubStatus(): NexusLinkStatus | null {
  return current;
}

export function useHubStatus(): NexusLinkStatus | null {
  return useSyncExternalStore(subscribe, hubStatus, hubStatus);
}
