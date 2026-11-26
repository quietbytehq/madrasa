import { useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

/**
 * Minimal accessible dropdown: a trigger button and a popover panel anchored to its logical
 * end. Closes on Escape, outside click, or when an item calls `close`.
 */
export interface MenuProps {
  trigger: (props: { open: boolean; toggle: () => void; id: string }) => ReactNode;
  children: (close: () => void) => ReactNode;
  label: string;
  width?: string;
  /**
   * What the panel is to a screen reader: a `menu` of actions to pick from (the default),
   * or a `region` that only shows something (notifications).
   */
  role?: "menu" | "region";
}

export function Menu({ trigger, children, label, width = "w-64", role = "menu" }: MenuProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const id = useId();

  useEffect(() => {
    if (!open) return;
    const root = rootRef.current;
    const opener = root?.querySelector<HTMLElement>("[aria-controls]") ?? null;
    const items = () =>
      Array.from(
        root?.querySelectorAll<HTMLElement>(`#${CSS.escape(id)} button, #${CSS.escape(id)} a`) ??
          [],
      ).filter((el) => !el.hasAttribute("disabled"));
    // Opened from the keyboard, the first entry is where the person is next; Up and Down
    // walk the entries; Escape closes and goes back to the button that opened the menu.
    if (opener && document.activeElement === opener) items()[0]?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        opener?.focus();
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End")
        return;
      const list = items();
      const at = list.indexOf(document.activeElement as HTMLElement);
      if (list.length === 0 || (at < 0 && !root?.contains(document.activeElement))) return;
      e.preventDefault();
      const to =
        e.key === "Home"
          ? 0
          : e.key === "End"
            ? list.length - 1
            : (at + (e.key === "ArrowDown" ? 1 : -1) + list.length) % list.length;
      list[to]?.focus();
    };
    const onClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onClick);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onClick);
    };
  }, [open, id]);

  return (
    <div ref={rootRef} className="relative">
      {trigger({ open, toggle: () => setOpen((o) => !o), id })}
      {open && (
        <div
          id={id}
          role={role}
          aria-label={label}
          className={cn(
            "absolute end-0 top-full z-40 mt-1 rounded-lg border border-border bg-card p-1 shadow-pop",
            width,
          )}
        >
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

export function MenuItem({
  children,
  onClick,
  icon,
  active,
}: {
  children: ReactNode;
  onClick?: () => void;
  icon?: ReactNode;
  active?: boolean;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      aria-current={active || undefined}
      className={cn(
        "flex h-8 w-full items-center gap-2 rounded px-2 text-start text-sm text-text hover:bg-hover",
        active && "bg-selection text-blue",
      )}
    >
      {icon && <span className="text-muted">{icon}</span>}
      <span className="flex-1 truncate">{children}</span>
    </button>
  );
}

export function MenuSection({ title, children }: { title?: ReactNode; children: ReactNode }) {
  return (
    <div role="group" className="py-1">
      {title && (
        <div className="px-2 pb-1 text-2xs font-semibold uppercase tracking-wide text-muted">
          {title}
        </div>
      )}
      {children}
    </div>
  );
}
