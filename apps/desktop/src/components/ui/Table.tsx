import { useMemo, useState } from "react";
import type { ReactNode } from "react";
import { ArrowDown, ArrowUp, ArrowUpDown, ChevronLeft, ChevronRight } from "lucide-react";
import { cn } from "@/lib/cn";
import { useI18n } from "@/i18n";
import { EmptyState } from "./EmptyState";
import { inputClass } from "./Input";
import { compareValues } from "@/lib/compare";

export type SortDirection = "asc" | "desc";

export interface Column<T> {
  key: string;
  header: ReactNode;
  /** Value used for sorting and filtering. Defaults to `row[key]`. */
  accessor?: (row: T) => string | number | boolean | null | undefined;
  render?: (row: T) => ReactNode;
  sortable?: boolean;
  align?: "start" | "end";
  width?: string;
}

export interface TableProps<T> {
  columns: Column<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  /** Show a filter box that matches any column's accessor text. */
  filterable?: boolean;
  /** Page size; 0 disables pagination. */
  pageSize?: number;
  pageSizeOptions?: number[];
  initialSort?: { key: string; direction: SortDirection };
  emptyTitle?: ReactNode;
  emptyDescription?: ReactNode;
  onRowClick?: (row: T) => void;
  className?: string;
  caption?: ReactNode;
}

function defaultAccessor<T>(row: T, key: string) {
  const value = (row as Record<string, unknown>)[key];
  return value as string | number | boolean | null | undefined;
}

export function Table<T>({
  columns,
  rows,
  rowKey,
  filterable = false,
  pageSize: initialPageSize = 25,
  pageSizeOptions = [10, 25, 50, 100],
  initialSort,
  emptyTitle,
  emptyDescription,
  onRowClick,
  className,
  caption,
}: TableProps<T>) {
  const { t, formatNumber } = useI18n();
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<{ key: string; direction: SortDirection } | null>(
    initialSort ?? null,
  );
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(initialPageSize);

  const accessorFor = (col: Column<T>) =>
    col.accessor ?? ((row: T) => defaultAccessor(row, col.key));

  const filtered = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return rows;
    return rows.filter((row) =>
      columns.some((col) => {
        const v = accessorFor(col)(row);
        return v != null && String(v).toLowerCase().includes(needle);
      }),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, columns, filter]);

  const sorted = useMemo(() => {
    if (!sort) return filtered;
    const col = columns.find((c) => c.key === sort.key);
    if (!col) return filtered;
    const acc = accessorFor(col);
    const sign = sort.direction === "asc" ? 1 : -1;
    return [...filtered].sort((a, b) => sign * compareValues(acc(a), acc(b)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filtered, sort, columns]);

  const paginated = pageSize > 0;
  const total = sorted.length;
  const pages = paginated ? Math.max(1, Math.ceil(total / pageSize)) : 1;
  const safePage = Math.min(page, pages);
  const start = paginated ? (safePage - 1) * pageSize : 0;
  const visible = paginated ? sorted.slice(start, start + pageSize) : sorted;

  const toggleSort = (col: Column<T>) => {
    if (!col.sortable) return;
    setSort((current) => {
      if (!current || current.key !== col.key) return { key: col.key, direction: "asc" };
      if (current.direction === "asc") return { key: col.key, direction: "desc" };
      return null;
    });
  };

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {filterable && (
        <input
          type="search"
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setPage(1);
          }}
          placeholder={t("table.filter")}
          aria-label={t("table.filter")}
          className={cn(inputClass, "max-w-xs")}
        />
      )}
      <div className="scrollbar-thin overflow-auto rounded-lg border border-border bg-card">
        <table className="w-full border-collapse text-sm">
          {caption && <caption className="sr-only">{caption}</caption>}
          <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
            <tr>
              {columns.map((col) => {
                const active = sort?.key === col.key;
                const ariaSort = active
                  ? sort!.direction === "asc"
                    ? "ascending"
                    : "descending"
                  : undefined;
                return (
                  <th
                    key={col.key}
                    scope="col"
                    aria-sort={col.sortable ? (ariaSort ?? "none") : undefined}
                    style={col.width ? { width: col.width } : undefined}
                    className={cn(
                      "border-b border-border px-3 py-2 font-semibold",
                      col.align === "end" ? "text-end" : "text-start",
                    )}
                  >
                    {col.sortable ? (
                      <button
                        type="button"
                        onClick={() => toggleSort(col)}
                        className="inline-flex items-center gap-1 rounded hover:text-text"
                      >
                        {col.header}
                        {active ? (
                          sort!.direction === "asc" ? (
                            <ArrowUp className="h-3 w-3" aria-label={t("table.sortAsc")} />
                          ) : (
                            <ArrowDown className="h-3 w-3" aria-label={t("table.sortDesc")} />
                          )
                        ) : (
                          <ArrowUpDown className="h-3 w-3 opacity-50" aria-hidden />
                        )}
                      </button>
                    ) : (
                      col.header
                    )}
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {visible.length === 0 ? (
              <tr>
                <td colSpan={columns.length}>
                  <EmptyState
                    title={emptyTitle ?? t("table.empty")}
                    description={emptyDescription}
                  />
                </td>
              </tr>
            ) : (
              visible.map((row) => (
                <tr
                  key={rowKey(row)}
                  onClick={onRowClick ? () => onRowClick(row) : undefined}
                  tabIndex={onRowClick ? 0 : undefined}
                  onKeyDown={
                    onRowClick
                      ? (e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            onRowClick(row);
                          }
                        }
                      : undefined
                  }
                  className={cn(
                    "border-b border-border last:border-b-0",
                    onRowClick && "cursor-pointer hover:bg-selection focus:bg-selection",
                  )}
                >
                  {columns.map((col) => (
                    <td
                      key={col.key}
                      className={cn(
                        "px-3 py-1.5 align-middle",
                        col.align === "end" ? "text-end" : "text-start",
                      )}
                    >
                      {col.render ? col.render(row) : String(accessorFor(col)(row) ?? "")}
                    </td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      {paginated && total > 0 && (
        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
          <span>
            {t("table.showing", {
              from: formatNumber(start + 1),
              to: formatNumber(Math.min(start + pageSize, total)),
              total: formatNumber(total),
            })}
          </span>
          <div className="flex items-center gap-2">
            <label className="flex items-center gap-1">
              {t("table.rowsPerPage")}
              <select
                value={pageSize}
                onChange={(e) => {
                  setPageSize(Number(e.target.value));
                  setPage(1);
                }}
                className="h-6 rounded border border-border bg-card px-1 text-xs text-text"
              >
                {pageSizeOptions.map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </select>
            </label>
            <span>
              {t("table.page", { page: formatNumber(safePage), pages: formatNumber(pages) })}
            </span>
            <button
              type="button"
              aria-label={t("table.previous")}
              disabled={safePage <= 1}
              onClick={() => setPage(safePage - 1)}
              className="rounded border border-border bg-card p-1 hover:bg-hover disabled:opacity-40"
            >
              <ChevronLeft className="h-3.5 w-3.5 rtl:rotate-180" aria-hidden />
            </button>
            <button
              type="button"
              aria-label={t("table.next")}
              disabled={safePage >= pages}
              onClick={() => setPage(safePage + 1)}
              className="rounded border border-border bg-card p-1 hover:bg-hover disabled:opacity-40"
            >
              <ChevronRight className="h-3.5 w-3.5 rtl:rotate-180" aria-hidden />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
