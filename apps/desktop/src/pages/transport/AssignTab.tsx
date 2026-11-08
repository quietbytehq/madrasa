import { useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import type { RouteDetail, TransportStudent } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useMoney } from "@/lib/useMoney";
import { errorMessage, useAsync } from "@/lib/useAsync";

function today(): string {
  return new Date().toISOString().slice(0, 10);
}

/** Students of a section with their stop; tick several and put them on a stop at once. */
export function AssignTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const money = useMoney();
  const toast = useToast();
  const years = useAsync(() => api.academicYears(), []);
  const active = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const [yearId, setYearId] = useState("");
  const year = yearId || active?.id || "";
  const sections = useAsync(
    () => (year ? api.sections({ academic_year_id: year }) : Promise.resolve([])),
    [year],
  );
  const [sectionId, setSectionId] = useState("");
  const [query, setQuery] = useState("");
  const [without, setWithout] = useState(false);
  const students = useAsync(
    () =>
      year
        ? api.transportStudents({
            academic_year_id: year,
            section_id: sectionId || undefined,
            q: query || undefined,
            without: without || undefined,
          })
        : Promise.resolve([]),
    [year, sectionId, query, without],
  );
  const routes = useAsync(() => api.transportRoutes(), []);
  const [routeId, setRouteId] = useState("");
  const route = useAsync<RouteDetail | null>(
    () => (routeId ? api.transportRoute(routeId) : Promise.resolve(null)),
    [routeId],
  );
  const [stopId, setStopId] = useState("");
  const [from, setFrom] = useState(today());
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const rows = students.data ?? [];
  const toggle = (id: string) => {
    const next = new Set(ticked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setTicked(next);
  };
  const assign = async () => {
    if (!stopId || ticked.size === 0) return;
    setBusy(true);
    try {
      const r = await api.assignTransportBulk({
        student_ids: [...ticked],
        academic_year_id: year,
        stop_id: stopId,
        from_date: from,
      });
      toast.push({
        kind: r.refused.length ? "warning" : "success",
        title: t("trp.assign.done", { n: r.assigned, refused: r.refused.length }),
        description:
          r.warning?.kind === "over"
            ? t("trp.capacity.over", { n: r.warning.by })
            : r.warning?.kind === "full"
              ? t("trp.capacity.full")
              : undefined,
      });
      setTicked(new Set());
      await students.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const end = async (s: TransportStudent) => {
    if (!s.assignment_id) return;
    setBusy(true);
    try {
      await api.updateTransportAssignment(s.assignment_id, { to_date: today() });
      toast.push({ kind: "success", title: t("trp.assign.ended") });
      await students.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card>
      <p className="mb-3 text-xs text-muted">{t("trp.assign.hint")}</p>
      <div className="mb-3 flex flex-wrap items-end gap-2">
        <Select
          label={t("tt.year")}
          value={year}
          onChange={(e) => setYearId(e.target.value)}
          options={(years.data ?? []).map((y) => ({ value: y.id, label: y.name }))}
          className="w-36"
        />
        <Select
          label={t("students.filter.section")}
          value={sectionId}
          onChange={(e) => setSectionId(e.target.value)}
          options={[
            { value: "", label: t("trp.assign.allSections") },
            ...(sections.data ?? []).map((s) => ({
              value: s.id,
              label: `${s.class_name}-${s.name}`,
            })),
          ]}
          className="w-40"
        />
        <Input
          label={t("action.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-48"
        />
        <label className="flex items-center gap-2 pb-2 text-sm">
          <input type="checkbox" checked={without} onChange={(e) => setWithout(e.target.checked)} />
          {t("trp.assign.withoutOnly")}
        </label>
      </div>
      {manage && (
        <div
          className="mb-3 flex flex-wrap items-end gap-2 rounded border border-border bg-selection/40 p-2"
          data-testid="assign-bar"
        >
          <Select
            label={t("trp.route")}
            value={routeId}
            onChange={(e) => {
              setRouteId(e.target.value);
              setStopId("");
            }}
            options={[
              { value: "", label: t("trp.assign.pickRoute") },
              ...(routes.data ?? [])
                .filter((r) => r.active)
                .map((r) => ({ value: r.id, label: r.name })),
            ]}
            className="w-44"
          />
          <Select
            label={t("trp.stop")}
            value={stopId}
            onChange={(e) => setStopId(e.target.value)}
            disabled={!routeId}
            options={[
              { value: "", label: t("trp.assign.pickStop") },
              ...(route.data?.stop_list ?? []).map((s) => ({ value: s.id, label: s.name })),
            ]}
            className="w-44"
          />
          <Input
            label={t("trp.assign.from")}
            type="date"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
            className="w-40"
          />
          <Button
            loading={busy}
            disabled={!stopId || ticked.size === 0}
            onClick={() => void assign()}
            data-testid="assign"
          >
            {t("trp.assign.button", { n: ticked.size })}
          </Button>
          {route.data && (
            <span className="pb-2 text-xs text-muted">
              {t("trp.students")}: {route.data.students}
              {route.data.vehicle_capacity !== null && ` / ${route.data.vehicle_capacity}`}
            </span>
          )}
        </div>
      )}
      {students.loading && !students.data ? (
        <Spinner />
      ) : rows.length === 0 ? (
        <EmptyState title={t("trp.assign.empty")} />
      ) : (
        <Table<TransportStudent>
          rows={rows}
          rowKey={(s) => s.student_id}
          pageSize={50}
          columns={[
            ...(manage
              ? [
                  {
                    key: "tick",
                    header: (
                      <input
                        type="checkbox"
                        aria-label={t("trp.assign.tickAll")}
                        checked={rows.length > 0 && ticked.size === rows.length}
                        onChange={(e) =>
                          setTicked(
                            e.target.checked ? new Set(rows.map((r) => r.student_id)) : new Set(),
                          )
                        }
                      />
                    ),
                    render: (s: TransportStudent) => (
                      <input
                        type="checkbox"
                        aria-label={s.full_name}
                        checked={ticked.has(s.student_id)}
                        onChange={() => toggle(s.student_id)}
                      />
                    ),
                  },
                ]
              : []),
            { key: "student_number", header: t("students.number") },
            { key: "full_name", header: t("students.fullName"), sortable: true },
            {
              key: "class_name",
              header: t("trp.class"),
              render: (s) => `${s.class_name}-${s.section_name}`,
            },
            {
              key: "route_name",
              header: t("trp.route"),
              render: (s) =>
                s.route_name ? (
                  <span>
                    {s.route_name} <span className="text-xs text-muted">· {s.stop_name}</span>
                  </span>
                ) : (
                  <Badge tone="warning">{t("trp.assign.none")}</Badge>
                ),
            },
            {
              key: "charge",
              header: t("trp.charge"),
              align: "end",
              render: (s) => (s.charge !== null ? money(s.charge) : ""),
            },
            {
              key: "actions",
              header: "",
              render: (s) =>
                manage &&
                s.assignment_id && (
                  <div className="flex justify-end">
                    <Button size="sm" variant="ghost" disabled={busy} onClick={() => void end(s)}>
                      {t("trp.assign.end")}
                    </Button>
                  </div>
                ),
            },
          ]}
        />
      )}
    </Card>
  );
}
