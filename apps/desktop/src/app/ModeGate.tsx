import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { getMode } from "@/app/mode";
import { setBaseUrl } from "@/api/client";
import { Spinner } from "@/components/ui/Spinner";
import { ModePage } from "@/pages/ModePage";

/**
 * On a fresh desktop install, ask how this PC takes part before anything touches the API
 * (the shell starts the database or the proxy only once the mode is chosen).
 */
export function ModeGate({ children }: { children: ReactNode }) {
  const [state, setState] = useState<"loading" | "ask" | "ready">("loading");
  useEffect(() => {
    let cancelled = false;
    getMode()
      .then((m) => !cancelled && setState(m ? "ready" : "ask"))
      .catch(() => !cancelled && setState("ready"));
    return () => {
      cancelled = true;
    };
  }, []);
  if (state === "loading") {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner size="lg" />
      </div>
    );
  }
  if (state === "ask") {
    return (
      <ModePage
        onChosen={() => {
          setBaseUrl(null);
          setState("ready");
        }}
      />
    );
  }
  return <>{children}</>;
}
