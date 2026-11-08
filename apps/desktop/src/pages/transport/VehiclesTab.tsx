import { useState } from "react";
import { api } from "@/api/endpoints";
import type { Vehicle, VehicleBody } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

const EMPTY: VehicleBody = {
  registration_no: "",
  make_model: "",
  capacity: null,
  driver_name: "",
  driver_phone: "",
  helper_name: "",
  helper_phone: "",
  active: true,
  notes: "",
  custom: {},
};

/** Vehicles with capacity, driver and helper. */
export function VehiclesTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const toast = useToast();
  const vehicles = useAsync(() => api.vehicles(), []);
  const [editing, setEditing] = useState<Vehicle | "new" | null>(null);
  const [form, setForm] = useState<VehicleBody>(EMPTY);
  const [busy, setBusy] = useState(false);
  const open = (v: Vehicle | "new") => {
    setForm(v === "new" ? EMPTY : { ...v });
    setEditing(v);
  };
  const save = async () => {
    setBusy(true);
    try {
      if (editing === "new") await api.createVehicle(form);
      else if (editing) await api.updateVehicle(editing.id, form);
      toast.push({ kind: "success", title: t("trp.vehicle.saved") });
      setEditing(null);
      await vehicles.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!editing || editing === "new" || !window.confirm(t("trp.vehicle.deleteConfirm"))) return;
    setBusy(true);
    try {
      await api.deleteVehicle(editing.id);
      setEditing(null);
      await vehicles.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const field = (k: keyof VehicleBody, label: string) => (
    <Input
      key={k}
      label={label}
      value={String(form[k] ?? "")}
      onChange={(e) => setForm({ ...form, [k]: e.target.value })}
    />
  );
  return (
    <Card
      actions={
        manage && (
          <Button size="sm" onClick={() => open("new")} data-testid="add-vehicle">
            {t("trp.vehicle.add")}
          </Button>
        )
      }
    >
      <p className="mb-3 text-xs text-muted">{t("trp.vehicle.hint")}</p>
      {vehicles.loading && !vehicles.data ? (
        <Spinner />
      ) : (vehicles.data ?? []).length === 0 ? (
        <EmptyState title={t("trp.vehicle.empty")} />
      ) : (
        <Table<Vehicle>
          rows={vehicles.data ?? []}
          rowKey={(v) => v.id}
          pageSize={0}
          onRowClick={manage ? (v) => open(v) : undefined}
          columns={[
            { key: "registration_no", header: t("trp.vehicle.registration"), sortable: true },
            { key: "make_model", header: t("trp.vehicle.makeModel") },
            {
              key: "capacity",
              header: t("trp.vehicle.capacity"),
              align: "end",
              render: (v) => v.capacity ?? "—",
            },
            {
              key: "driver_name",
              header: t("trp.vehicle.driver"),
              render: (v) => (
                <span>
                  {v.driver_name} <span className="text-xs text-muted">{v.driver_phone}</span>
                </span>
              ),
            },
            {
              key: "helper_name",
              header: t("trp.vehicle.helper"),
              render: (v) => (
                <span>
                  {v.helper_name} <span className="text-xs text-muted">{v.helper_phone}</span>
                </span>
              ),
            },
            {
              key: "active",
              header: t("trp.status"),
              render: (v) => (
                <Badge tone={v.active ? "success" : "neutral"}>
                  {t(v.active ? "trp.active" : "trp.inactive")}
                </Badge>
              ),
            },
          ]}
        />
      )}
      {editing && (
        <Dialog
          open
          onClose={() => setEditing(null)}
          title={t(editing === "new" ? "trp.vehicle.add" : "trp.vehicle.edit")}
          footer={
            <div className="flex w-full justify-between gap-2">
              <span>
                {editing !== "new" && (
                  <Button variant="ghost" disabled={busy} onClick={() => void remove()}>
                    {t("action.delete")}
                  </Button>
                )}
              </span>
              <span className="flex gap-2">
                <Button variant="secondary" onClick={() => setEditing(null)}>
                  {t("action.cancel")}
                </Button>
                <Button loading={busy} onClick={() => void save()} data-testid="save-vehicle">
                  {t("action.save")}
                </Button>
              </span>
            </div>
          }
        >
          <div className="grid grid-cols-2 gap-3">
            {field("registration_no", t("trp.vehicle.registration"))}
            {field("make_model", t("trp.vehicle.makeModel"))}
            <Input
              label={t("trp.vehicle.capacity")}
              type="number"
              min={0}
              value={form.capacity ?? ""}
              onChange={(e) =>
                setForm({
                  ...form,
                  capacity: e.target.value === "" ? null : Number(e.target.value),
                })
              }
            />
            <label className="flex items-end gap-2 pb-2 text-sm">
              <input
                type="checkbox"
                checked={form.active}
                onChange={(e) => setForm({ ...form, active: e.target.checked })}
              />
              {t("trp.active")}
            </label>
            {field("driver_name", t("trp.vehicle.driver"))}
            {field("driver_phone", t("trp.vehicle.driverPhone"))}
            {field("helper_name", t("trp.vehicle.helper"))}
            {field("helper_phone", t("trp.vehicle.helperPhone"))}
          </div>
          <Input
            label={t("trp.notes")}
            value={form.notes}
            onChange={(e) => setForm({ ...form, notes: e.target.value })}
            className="mt-3"
          />
        </Dialog>
      )}
    </Card>
  );
}
