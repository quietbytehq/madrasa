import { createContext, useContext } from "react";
import type { HubPermission, Me } from "./api";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/useAsync";

/** Who is signed in to the hub console, and what they may do. */
export interface HubSession {
  me: Me;
  can: (permission: HubPermission) => boolean;
  signOut: () => void;
}

export const HubSessionContext = createContext<HubSession | null>(null);

export function useHubSession(): HubSession {
  const ctx = useContext(HubSessionContext);
  if (!ctx) throw new Error("useHubSession must be used inside the console shell");
  return ctx;
}

/** Show a failed call as a toast: the hub's own words, or a plain line when it is away. */
export function useFail(): (err: unknown) => void {
  const toast = useToast();
  const { t } = useI18n();
  return (err) => toast.push({ kind: "error", title: errorMessage(err, t("hub.error.network")) });
}
