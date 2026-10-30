import { Fragment } from "react";
import type { ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { Link } from "react-router-dom";
import { cn } from "@/lib/cn";

export interface Crumb {
  label: ReactNode;
  /** Route path; the last crumb is usually the current page and has no `to`. */
  to?: string;
}

export function Breadcrumbs({ items, className }: { items: Crumb[]; className?: string }) {
  return (
    <nav aria-label="Breadcrumb" className={cn("text-xs text-muted", className)}>
      <ol className="flex flex-wrap items-center gap-1">
        {items.map((item, i) => {
          const last = i === items.length - 1;
          return (
            <Fragment key={i}>
              <li>
                {item.to && !last ? (
                  <Link to={item.to} className="rounded hover:text-text hover:underline">
                    {item.label}
                  </Link>
                ) : (
                  <span
                    aria-current={last ? "page" : undefined}
                    className={cn(last && "font-medium text-text")}
                  >
                    {item.label}
                  </span>
                )}
              </li>
              {!last && (
                <li aria-hidden>
                  <ChevronRight className="h-3.5 w-3.5 rtl:rotate-180" />
                </li>
              )}
            </Fragment>
          );
        })}
      </ol>
    </nav>
  );
}
