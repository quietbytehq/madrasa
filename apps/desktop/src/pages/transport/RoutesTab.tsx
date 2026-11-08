import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { RouteBody, RouteView, StopBody } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useMoney } from "@/lib/useMoney";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { time } from "./common";

const EMPTY: RouteBody = {
  name: "",
  code: "",
  vehicle_id: null,
  monthly_fee: 0,
  active: true,
  notes: "",
  stops: [],
};

/** Routes with their vehicle, stops and students; a dialog edits the route and its stops. */
export function RoutesTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const money = useMoney();
  const routes = useAsync(() => api.transportRoutes(), []);
  const [open, setOpen] = useState<string | "new" | null>(null);
  return (
    <Card
      actions={
        manage && (
          <Button size="sm" onClick={() => setOpen("new")} data-testid="add-route">
            {t("trp.route.add")}
          </Button>
        )
      }
    >
      <p className="mb-3 text-xs text-muted">{t("trp.route.hint")}</p>
      {routes.loading && !routes.data ? (
        <Spinner />
      ) : (routes.data ?? []).length === 0 ? (
        <EmptyState title={t("trp.route.empty")} />
      ) : (
        <Table<RouteView>
          rows={routes.data ?? []}
          rowKey={(r) => r.id}
          pageSize={0}
          onRowClick={(r) => setOpen(r.id)}
          columns={[
            { key: "name", header: t("trp.route.name"), sortable: true },
            {
              key: "vehicle_registration",
              header: t("trp.vehicle"),
              render: (r) => (
                <span>
                  {r.vehicle_registration ?? "—"}{" "}
                  {r.driver_name && <span className="text-xs text-muted">{r.driver_name}</span>}
                </span>
              ),
            },
            { key: "stops", header: t("trp.route.stops"), align: "end" },
            {
              key: "students",
              header: t("trp.students"),
              align: "end",
              render: (r) => (
                <Badge
                  tone={
                    r.vehicle_capacity !== null && r.students > r.vehicle_capacity
                      ? "danger"
                      : "neutral"
                  }
                >
                  {r.students}
                  {r.vehicle_capacity !== null && ` / ${r.vehicle_capacity}`}
                </Badge>
              ),
            },
            {
              key: "monthly_fee",
              header: t("trp.route.fee"),
              align: "end",
              render: (r) => money(r.monthly_fee),
            },
            {
              key: "active",
              header: t("trp.status"),
              render: (r) => (
                <Badge tone={r.active ? "success" : "neutral"}>
                  {t(r.active ? "trp.active" : "trp.inactive")}
                </Badge>
              ),
            },
          ]}
        />
      )}
      {open && (
        <RouteDialog
          id={open === "new" ? null : open}
          manage={manage}
          onClose={() => setOpen(null)}
          onChanged={() => void routes.reload()}
        />
      )}
    </Card>
  );
}

function RouteDialog({
  id,
  manage,
  onClose,
  onChanged,
}: {
  id: string | null;
  manage: boolean;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const vehicles = useAsync(() => api.vehicles(), []);
  const detail = useAsync(() => (id ? api.transportRoute(id) : Promise.resolve(null)), [id]);
  const [form, setForm] = useState<RouteBody>(EMPTY);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const d = detail.data;
    if (d) {
      setForm({
        name: d.name,
        code: d.code,
        vehicle_id: d.vehicle_id,
        monthly_fee: d.monthly_fee,
        active: d.active,
        notes: d.notes,
        stops: d.stop_list.map((s) => ({
          id: s.id,
          name: s.name,
          pickup_time: s.pickup_time,
          drop_time: s.drop_time,
          monthly_fee: s.monthly_fee,
        })),
      });
    }
  }, [detail.data]);
  const setStop = (i: number, patch: Partial<StopBody>) =>
    setForm({ ...form, stops: form.stops.map((s, j) => (j === i ? { ...s, ...patch } : s)) });
  const move = (i: number, dir: -1 | 1) => {
    const stops = [...form.stops];
    const j = i + dir;
    if (j < 0 || j >= stops.length) return;
    [stops[i], stops[j]] = [stops[j]!, stops[i]!];
    setForm({ ...form, stops });
  };
  const save = async () => {
    setBusy(true);
    try {
      const body: RouteBody = {
        ...form,
        monthly_fee: Number(form.monthly_fee) || 0,
        stops: form.stops.map((s) => ({
          ...s,
          pickup_time: s.pickup_time ? s.pickup_time.slice(0, 5) + ":00" : null,
          drop_time: s.drop_time ? s.drop_time.slice(0, 5) + ":00" : null,
          monthly_fee:
            s.monthly_fee === null || s.monthly_fee === "" ? null : Number(s.monthly_fee),
        })),
      };
      if (id) await api.updateTransportRoute(id, body);
      else await api.createTransportRoute(body);
      toast.push({ kind: "success", title: t("trp.route.saved") });
      onChanged();
      onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!id || !window.confirm(t("trp.route.deleteConfirm"))) return;
    setBusy(true);
    try {
      await api.deleteTransportRoute(id);
      onChanged();
      onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  if (id && !detail.data) {
    return (
      <Dialog open onClose={onClose} title={t("trp.route.edit")}>
        <Spinner />
      </Dialog>
    );
  }
  return (
    <Dialog
      open
      onClose={onClose}
      size="lg"
      title={id ? t("trp.route.edit") : t("trp.route.add")}
      footer={
        manage && (
          <div className="flex w-full justify-between gap-2">
            <span>
              {id && (
                <Button variant="ghost" disabled={busy} onClick={() => void remove()}>
                  {t("action.delete")}
                </Button>
              )}
            </span>
            <span className="flex gap-2">
              <Button variant="secondary" onClick={onClose}>
                {t("action.cancel")}
              </Button>
              <Button loading={busy} onClick={() => void save()} data-testid="save-route">
                {t("action.save")}
              </Button>
            </span>
          </div>
        )
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Input
          label={t("trp.route.name")}
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          disabled={!manage}
        />
        <Input
          label={t("trp.route.code")}
          value={form.code}
          onChange={(e) => setForm({ ...form, code: e.target.value })}
          disabled={!manage}
        />
        <Select
          label={t("trp.vehicle")}
          value={form.vehicle_id ?? ""}
          onChange={(e) => setForm({ ...form, vehicle_id: e.target.value || null })}
          disabled={!manage}
          options={[
            { value: "", label: t("trp.route.noVehicle") },
            ...(vehicles.data ?? []).map((v) => ({
              value: v.id,
              label: `${v.registration_no}${v.capacity !== null ? ` (${v.capacity})` : ""}`,
            })),
          ]}
        />
        <Input
          label={t("trp.route.fee")}
          type="number"
          min={0}
          step="1"
          value={String(form.monthly_fee)}
          onChange={(e) => setForm({ ...form, monthly_fee: e.target.value })}
          disabled={!manage}
        />
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={form.active}
            disabled={!manage}
            onChange={(e) => setForm({ ...form, active: e.target.checked })}
          />
          {t("trp.active")}
        </label>
        <Input
          label={t("trp.notes")}
          value={form.notes}
          onChange={(e) => setForm({ ...form, notes: e.target.value })}
          disabled={!manage}
        />
      </div>
      <div className="mb-1 mt-4 flex items-center justify-between">
        <span className="text-xs font-medium uppercase tracking-wide text-muted">
          {t("trp.route.stops")}
        </span>
        {manage && (
          <Button
            size="sm"
            variant="secondary"
            onClick={() =>
              setForm({
                ...form,
                stops: [
                  ...form.stops,
                  { name: "", pickup_time: null, drop_time: null, monthly_fee: null },
                ],
              })
            }
            data-testid="add-stop"
          >
            {t("trp.stop.add")}
          </Button>
        )}
      </div>
      <p className="mb-2 text-xs text-muted">{t("trp.stop.hint")}</p>
      {form.stops.length === 0 ? (
        <p className="text-sm text-muted">{t("trp.stop.none")}</p>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-start text-xs text-muted">
              <th className="py-1 text-start">#</th>
              <th className="py-1 text-start">{t("trp.stop.name")}</th>
              <th className="py-1 text-start">{t("trp.stop.pickup")}</th>
              <th className="py-1 text-start">{t("trp.stop.drop")}</th>
              <th className="py-1 text-start">{t("trp.stop.fee")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {form.stops.map((s, i) => (
              <tr key={s.id ?? `new-${i}`}>
                <td className="py-1 pe-2 text-muted">{i + 1}</td>
                <td className="py-1 pe-2">
                  <Input
                    aria-label={`${t("trp.stop.name")} ${i + 1}`}
                    value={s.name}
                    onChange={(e) => setStop(i, { name: e.target.value })}
                    disabled={!manage}
                  />
                </td>
                <td className="py-1 pe-2">
                  <Input
                    type="time"
                    value={time(s.pickup_time)}
                    onChange={(e) => setStop(i, { pickup_time: e.target.value || null })}
                    disabled={!manage}
                    className="w-28"
                  />
                </td>
                <td className="py-1 pe-2">
                  <Input
                    type="time"
                    value={time(s.drop_time)}
                    onChange={(e) => setStop(i, { drop_time: e.target.value || null })}
                    disabled={!manage}
                    className="w-28"
                  />
                </td>
                <td className="py-1 pe-2">
                  <Input
                    type="number"
                    min={0}
                    placeholder={t("trp.stop.routeFee")}
                    value={s.monthly_fee === null ? "" : String(s.monthly_fee)}
                    onChange={(e) =>
                      setStop(i, { monthly_fee: e.target.value === "" ? null : e.target.value })
                    }
                    disabled={!manage}
                    className="w-28"
                  />
                </td>
                <td className="whitespace-nowrap py-1 text-end">
                  {manage && (
                    <>
                      <Button size="sm" variant="ghost" onClick={() => move(i, -1)} aria-label="↑">
                        ↑
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => move(i, 1)} aria-label="↓">
                        ↓
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                          setForm({ ...form, stops: form.stops.filter((_, j) => j !== i) })
                        }
                      >
                        ×
                      </Button>
                    </>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Dialog>
  );
}
