import { useEffect } from "react";

/**
 * Arrow keys on every tab strip of the app: with a tab focused, Left and Right (mirrored in
 * right-to-left), Home and End move to and open the neighbouring tab, as a keyboard user
 * expects of tabs. One listener for the whole app, so no tab strip can forget it.
 */
/** Counts key presses, so a late "stay on the strip" is dropped once the person has moved on. */
let presses = 0;

export function useTabKeys() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Any other key pressed anywhere means the person has moved on.
      presses += 1;
      const target = e.target as HTMLElement | null;
      if (!target || target.getAttribute("role") !== "tab") return;
      if (e.altKey || e.ctrlKey || e.metaKey) return;
      const strip = target.closest('[role="tablist"]');
      if (!strip) return;
      const tabs = Array.from(strip.querySelectorAll<HTMLElement>('[role="tab"]:not([disabled])'));
      const at = tabs.indexOf(target);
      if (at < 0) return;
      const step = getComputedStyle(strip).direction === "rtl" ? -1 : 1;
      const moves: Record<string, number> = {
        ArrowRight: at + step,
        ArrowLeft: at - step,
        Home: 0,
        End: tabs.length - 1,
      };
      const to = moves[e.key];
      if (to === undefined) return;
      e.preventDefault();
      const index = (to + tabs.length) % tabs.length;
      tabs[index]?.click();
      // The keyboard stays on the tab strip, also when what the tab shows would take the
      // focus for itself (a search box that focuses itself when it appears, a moment later).
      const stay = () =>
        strip.querySelectorAll<HTMLElement>('[role="tab"]:not([disabled])')[index]?.focus();
      stay();
      const epoch = presses;
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          if (epoch === presses && !strip.contains(document.activeElement)) stay();
        }),
      );
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);
}
