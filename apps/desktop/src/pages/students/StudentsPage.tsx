import { useEffect, useMemo, useState } from "react";
import { Plus, UserRound } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { api } from "@/api/endpoints";
import type { StudentListQuery, StudentListRow } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { classLabel, STATUSES, StatusBadge, statusLabel } from "./common";

const PAGE_SIZE = 50;

export function StudentsPage() {
  const { t, formatNumber } = useI18n();
  const { can } = useSession();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const q = params.get("q") ?? "";
  const classId = params.get("class_id") ?? "";
  const sectionId = params.get("section_id") ?? "";
  const status = params.get("status") ?? "active";
  const sort = (params.get("sort") ?? "name") as NonNullable<StudentListQuery["sort"]>;
  const order = (params.get("order") ?? "asc") as "asc" | "desc";
  const page = Number(params.get("page") ?? "1");
  const [search, setSearch] = useState(q);

  const set = (patch: Record<string, string>) => {
    const next = new URLSearchParams(params);
    for (const [k, v] of Object.entries(patch)) {
      if (v) next.set(k, v);
      else next.delete(k);
    }
    if (!("page" in patch)) next.delete("page");
    setParams(next, { replace: true });
  };

  // Debounced search box → URL.
  useEffect(() => {
    const id = setTimeout(() => {
      if (search !== q) set({ q: search });
    }, 250);
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search]);

  const classes = useAsync(() => api.classes(), []);
  const sections = useAsync(() => api.sections(classId ? { class_id: classId } : {}), [classId]);
  const list = useAsync(
    () =>
      api.students({
        q: q || undefined,
        class_id: classId || undefined,
        section_id: sectionId || undefined,
        status: (status || undefined) as StudentListQuery["status"],
        sort,
        order,
        page,
        page_size: PAGE_SIZE,
      }),
    [q, classId, sectionId, status, sort, order, page],
  );

  const activeYearSections = useMemo(() => sections.data ?? [], [sections.data]);
  const total = list.data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE));

  const toggleSort = (key: NonNullable<StudentListQuery["sort"]>) => {
    if (sort === key) set({ order: order === "asc" ? "desc" : "asc" });
    else set({ sort: key, order: "asc" });
  };

  const columns: Array<{
    key: NonNullable<StudentListQuery["sort"]> | "father" | "status";
    label: string;
    sortable: boolean;
    align?: "end";
  }> = [
    { key: "number", label: t("students.number"), sortable: true },
    { key: "name", label: t("students.fullName"), sortable: true },
    { key: "father", label: t("students.fatherName"), sortable: false },
    { key: "class", label: t("students.classSection"), sortable: true },
    { key: "admission_date", label: t("students.admissionDate"), sortable: true },
    { key: "status", label: t("common.status"), sortable: false },
  ];

  return (
    <>
      <PageHeader
        title={t("students.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("students.title") }]}
        actions={
          can("students.manage") && (
            <Button
              icon={<Plus className="h-4 w-4" aria-hidden />}
              onClick={() => navigate("/students/new")}
            >
              {t("students.add")}
            </Button>
          )
        }
      />
      <div className="mb-3 flex flex-wrap items-end gap-2">
        <Input
          type="search"
          aria-label={t("students.search")}
          placeholder={t("students.search")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="w-80"
        />
        <Select
          aria-label={t("students.filter.class")}
          value={classId}
          onChange={(e) => set({ class_id: e.target.value, section_id: "" })}
          options={[
            { value: "", label: `${t("students.filter.class")}: ${t("common.all")}` },
            ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
          ]}
          className="w-40"
        />
        <Select
          aria-label={t("students.filter.section")}
          value={sectionId}
          onChange={(e) => set({ section_id: e.target.value })}
          options={[
            { value: "", label: `${t("students.filter.section")}: ${t("common.all")}` },
            ...activeYearSections.map((s) => ({ value: s.id, label: `${s.class_name}-${s.name}` })),
          ]}
          className="w-44"
        />
        <Select
          aria-label={t("students.filter.status")}
          value={status}
          onChange={(e) => set({ status: e.target.value })}
          options={[
            { value: "", label: `${t("students.filter.status")}: ${t("common.all")}` },
            ...STATUSES.map((s) => ({ value: s, label: statusLabel(t, s) })),
          ]}
          className="w-40"
        />
        <span className="ms-auto text-xs text-muted">
          {t("common.results", { count: formatNumber(total) })}
        </span>
      </div>

      <div className="scrollbar-thin overflow-auto rounded-lg border border-border bg-card">
        <table className="w-full border-collapse text-sm">
          <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
            <tr>
              <th scope="col" className="w-10 border-b border-border px-3 py-2" />
              {columns.map((c) => (
                <th
                  key={c.key}
                  scope="col"
                  aria-sort={
                    c.sortable
                      ? sort === c.key
                        ? order === "asc"
                          ? "ascending"
                          : "descending"
                        : "none"
                      : undefined
                  }
                  className={cn(
                    "border-b border-border px-3 py-2 text-start font-semibold",
                    c.align === "end" && "text-end",
                  )}
                >
                  {c.sortable ? (
                    <button
                      type="button"
                      onClick={() => toggleSort(c.key as NonNullable<StudentListQuery["sort"]>)}
                      className="rounded hover:text-text"
                    >
                      {c.label}
                      {sort === c.key ? (order === "asc" ? " ↑" : " ↓") : ""}
                    </button>
                  ) : (
                    c.label
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {list.loading && !list.data ? (
              <tr>
                <td colSpan={7} className="py-10 text-center">
                  <Spinner />
                </td>
              </tr>
            ) : list.error ? (
              <tr>
                <td colSpan={7}>
                  <EmptyState
                    title={errorMessage(list.error, t("common.error.load"))}
                    action={
                      <Button variant="secondary" onClick={() => void list.reload()}>
                        {t("action.retry")}
                      </Button>
                    }
                  />
                </td>
              </tr>
            ) : (list.data?.items.length ?? 0) === 0 ? (
              <tr>
                <td colSpan={7}>
                  <EmptyState
                    title={t("students.empty")}
                    description={
                      !q && !classId && !sectionId && status === "active"
                        ? t("students.empty.first")
                        : undefined
                    }
                  />
                </td>
              </tr>
            ) : (
              list.data!.items.map((s) => (
                <Row key={s.id} s={s} onOpen={() => navigate(`/students/${s.id}`)} />
              ))
            )}
          </tbody>
        </table>
      </div>
      {total > PAGE_SIZE && (
        <div className="mt-2 flex items-center justify-end gap-2 text-xs text-muted">
          <span>{t("table.page", { page: formatNumber(page), pages: formatNumber(pages) })}</span>
          <Button
            size="sm"
            variant="secondary"
            disabled={page <= 1}
            onClick={() => set({ page: String(page - 1) })}
          >
            {t("table.previous")}
          </Button>
          <Button
            size="sm"
            variant="secondary"
            disabled={page >= pages}
            onClick={() => set({ page: String(page + 1) })}
          >
            {t("table.next")}
          </Button>
        </div>
      )}
    </>
  );
}

function Row({ s, onOpen }: { s: StudentListRow; onOpen: () => void }) {
  const { t, formatDate } = useI18n();
  return (
    <tr
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
      className="cursor-pointer border-b border-border last:border-b-0 hover:bg-selection focus:bg-selection"
    >
      <td className="px-3 py-1.5">
        <span className="flex h-7 w-7 items-center justify-center rounded-full bg-lavender text-navy">
          <UserRound className="h-3.5 w-3.5" aria-hidden />
        </span>
      </td>
      <td className="px-3 py-1.5 font-mono text-xs text-muted">{s.student_number}</td>
      <td className="px-3 py-1.5 font-medium text-text">{s.full_name}</td>
      <td className="px-3 py-1.5">{s.father_name}</td>
      <td className="px-3 py-1.5">
        {classLabel(s.class_name, s.section_name)}
        {s.roll_no != null && <span className="ms-1 text-xs text-muted">#{s.roll_no}</span>}
      </td>
      <td className="px-3 py-1.5">{formatDate(s.admission_date)}</td>
      <td className="px-3 py-1.5">
        <StatusBadge status={s.status} t={t} />
      </td>
    </tr>
  );
}
