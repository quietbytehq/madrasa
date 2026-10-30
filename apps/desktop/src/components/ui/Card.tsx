import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/cn";

export interface CardProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  title?: ReactNode;
  /** Right-aligned (logical end) actions in the header. */
  actions?: ReactNode;
  footer?: ReactNode;
  padded?: boolean;
}

export function Card({
  title,
  actions,
  footer,
  padded = true,
  className,
  children,
  ...rest
}: CardProps) {
  return (
    <section
      className={cn("flex flex-col rounded-lg border border-border bg-card shadow-card", className)}
      {...rest}
    >
      {(title || actions) && (
        <header className="flex items-center justify-between gap-2 border-b border-border px-4 py-2.5">
          {title && <h2 className="text-sm font-semibold text-navy">{title}</h2>}
          {actions && <div className="flex items-center gap-2">{actions}</div>}
        </header>
      )}
      <div className={cn("flex-1", padded && "p-4")}>{children}</div>
      {footer && (
        <footer className="border-t border-border px-4 py-2.5 text-xs text-muted">{footer}</footer>
      )}
    </section>
  );
}
