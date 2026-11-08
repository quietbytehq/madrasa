import { useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { useSchool } from "@/app/session";
import { useAsync } from "@/lib/useAsync";
import { CapacityBadge, time } from "./common";

/** One route's students by stop, printable for the driver. */
export function RosterTab() {
  const { t } = useI18n();
  const school = useSchool();
  const years = useAsync(() => api.academicYears(), []);
  const active = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const [yearId, setYearId] = useState("");
  const year = yearId || active?.id || "";
  const routes = useAsync(() => api.transportRoutes(), []);
  const [routeId, setRouteId] = useState("");
  const route = routeId || routes.data?.[0]?.id || "";
  const roster = useAsync(
    () =>
      route && year
        ? api.transportRoster(route, { academic_year_id: year })
        : Promise.resolve(null),
    [route, year],
  );
  const r = roster.data;
  return (
    <Card
      actions={
        <div className="flex flex-wrap items-end gap-2 print:hidden">
          <Select
            label={t("tt.year")}
            value={year}
            onChange={(e) => setYearId(e.target.value)}
            options={(years.data ?? []).map((y) => ({ value: y.id, label: y.name }))}
            className="w-36"
          />
          <Select
            label={t("trp.route")}
            value={route}
            onChange={(e) => setRouteId(e.target.value)}
            options={(routes.data ?? []).map((x) => ({ value: x.id, label: x.name }))}
            className="w-44"
          />
          <Button variant="secondary" onClick={() => window.print()} disabled={!r}>
            {t("action.print")}
          </Button>
        </div>
      }
    >
      {roster.loading && !r ? (
        <Spinner />
      ) : !r ? (
        <EmptyState title={t("trp.route.empty")} />
      ) : (
        <div data-testid="roster">
          <div className="mb-3 flex flex-wrap items-center gap-2">
            <span className="text-base font-semibold">
              {school.name} · {r.name}
            </span>
            {r.vehicle_registration && (
              <span className="text-sm text-muted">
                {r.vehicle_registration}
                {r.driver_name && ` · ${r.driver_name}`}
              </span>
            )}
            <span className="text-sm text-muted">
              {t("trp.students")}: {r.total}
            </span>
            <CapacityBadge warning={r.warning} seatsLeft={r.seats_left} />
          </div>
          {r.by_stop.map((s, i) => (
            <div key={s.stop.id} className="mb-3">
              <div className="mb-1 flex items-baseline gap-2 border-b border-border pb-1">
                <span className="font-medium">
                  {i + 1}. {s.stop.name}
                </span>
                {s.stop.pickup_time && (
                  <span className="text-xs text-muted">
                    {t("trp.stop.pickup")} {time(s.stop.pickup_time)}
                  </span>
                )}
                <span className="ms-auto text-xs text-muted">{s.students.length}</span>
              </div>
              {s.students.length === 0 ? (
                <p className="text-xs text-muted">{t("trp.roster.noStudents")}</p>
              ) : (
                <table className="w-full text-sm">
                  <tbody>
                    {s.students.map((a) => (
                      <tr key={a.id} className="border-b border-border/50">
                        <td className="py-1 pe-2 text-muted">{a.student_number}</td>
                        <td className="py-1 pe-2 font-medium">{a.student_name}</td>
                        <td className="py-1 pe-2">
                          {a.class_name}-{a.section_name}
                        </td>
                        <td className="py-1 pe-2 text-muted">{a.guardian_phone}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
