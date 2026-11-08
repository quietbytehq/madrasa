import { useState } from "react";
import { api } from "@/api/endpoints";
import type { AssetBody, AssetStatus, AssetView, InventoryRecipient } from "@/api/types";
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

const STATUSES: AssetStatus[] = ["in_store", "in_use", "under_repair", "disposed"];
const TONES: Record<AssetStatus, "neutral" | "success" | "warning" | "danger"> = {
  in_store: "neutral",
  in_use: "success",
  under_repair: "warning",
  disposed: "danger",
};

/** Tagged durable items: where each one is and who has it. */
export function AssetsTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const money = useMoney();
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("");
  const assets = useAsync(
    () => api.inventoryAssets({ q: query || undefined, status: status || undefined }),
    [query, status],
  );
  const [editing, setEditing] = useState<AssetView | "new" | null>(null);
  return (
    <Card
      actions={
        manage && (
          <Button size="sm" onClick={() => setEditing("new")} data-testid="add-asset">
            {t("inv.asset.add")}
          </Button>
        )
      }
    >
      <p className="mb-3 text-xs text-muted">{t("inv.asset.hint")}</p>
      <div className="mb-3 flex flex-wrap items-end gap-2">
        <Input
          label={t("action.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-56"
        />
        <Select
          label={t("inv.asset.status")}
          value={status}
          onChange={(e) => setStatus(e.target.value)}
          options={[
            { value: "", label: t("inv.asset.anyStatus") },
            ...STATUSES.map((s) => ({ value: s, label: t(`inv.asset.status.${s}`) })),
          ]}
          className="w-40"
        />
      </div>
      {assets.loading && !assets.data ? (
        <Spinner />
      ) : (assets.data ?? []).length === 0 ? (
        <EmptyState title={t("inv.asset.empty")} />
      ) : (
        <Table<AssetView>
          rows={assets.data ?? []}
          rowKey={(a) => a.id}
          pageSize={50}
          onRowClick={manage ? (a) => setEditing(a) : undefined}
          columns={[
            { key: "tag", header: t("inv.asset.tag"), sortable: true },
            { key: "item_name", header: t("inv.item"), sortable: true },
            { key: "serial_no", header: t("inv.asset.serial") },
            {
              key: "status",
              header: t("inv.asset.status"),
              render: (a) => (
                <Badge tone={TONES[a.status]}>{t(`inv.asset.status.${a.status}`)}</Badge>
              ),
            },
            {
              key: "location_name",
              header: t("inv.location"),
              render: (a) => a.location_name ?? "",
            },
            { key: "assigned_name", header: t("inv.asset.assignedTo") },
            {
              key: "cost",
              header: t("inv.asset.cost"),
              align: "end",
              render: (a) => (a.cost ? money(a.cost) : ""),
            },
          ]}
        />
      )}
      {editing && (
        <AssetDialog
          asset={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onChanged={() => void assets.reload()}
        />
      )}
    </Card>
  );
}

function AssetDialog({
  asset,
  onClose,
  onChanged,
}: {
  asset: AssetView | null;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const items = useAsync(() => api.inventoryItems({ active: true }), []);
  const locations = useAsync(() => api.inventoryLocations(), []);
  const [form, setForm] = useState<AssetBody>(
    asset
      ? {
          item_id: asset.item_id,
          tag: asset.tag,
          serial_no: asset.serial_no,
          location_id: asset.location_id,
          status: asset.status,
          assigned_kind: asset.assigned_kind,
          assigned_id: asset.assigned_id,
          assigned_name: asset.assigned_name,
          purchased_on: asset.purchased_on,
          cost: asset.cost,
          notes: asset.notes,
        }
      : {
          item_id: "",
          tag: "",
          serial_no: "",
          location_id: null,
          status: "in_store",
          assigned_kind: null,
          assigned_id: null,
          assigned_name: "",
          purchased_on: null,
          cost: null,
          notes: "",
        },
  );
  const [query, setQuery] = useState("");
  const matches = useAsync(
    () =>
      query.trim().length >= 2 ? api.inventoryRecipients({ q: query.trim() }) : Promise.resolve([]),
    [query],
  );
  const [busy, setBusy] = useState(false);
  const pick = (r: InventoryRecipient) => {
    setForm({
      ...form,
      assigned_kind: r.kind,
      assigned_id: r.id,
      assigned_name: r.name,
      status: "in_use",
    });
    setQuery("");
  };
  const save = async () => {
    setBusy(true);
    try {
      const body: AssetBody = {
        ...form,
        cost: form.cost === "" ? null : form.cost,
        purchased_on: form.purchased_on || null,
      };
      if (asset) await api.updateAsset(asset.id, body);
      else await api.createAsset(body);
      toast.push({ kind: "success", title: t("inv.asset.saved") });
      onChanged();
      onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!asset || !window.confirm(t("inv.asset.deleteConfirm"))) return;
    setBusy(true);
    try {
      await api.deleteAsset(asset.id);
      onChanged();
      onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={asset ? t("inv.asset.edit") : t("inv.asset.add")}
      footer={
        <div className="flex w-full justify-between gap-2">
          <span>
            {asset && (
              <Button variant="ghost" disabled={busy} onClick={() => void remove()}>
                {t("action.delete")}
              </Button>
            )}
          </span>
          <span className="flex gap-2">
            <Button variant="secondary" onClick={onClose}>
              {t("action.cancel")}
            </Button>
            <Button
              loading={busy}
              disabled={!form.item_id || !form.tag.trim()}
              onClick={() => void save()}
              data-testid="save-asset"
            >
              {t("action.save")}
            </Button>
          </span>
        </div>
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Select
          label={t("inv.item")}
          value={form.item_id}
          onChange={(e) => setForm({ ...form, item_id: e.target.value })}
          options={[
            { value: "", label: t("inv.move.pickItem") },
            ...(items.data ?? [])
              .filter((i) => i.is_asset || i.id === form.item_id)
              .map((i) => ({ value: i.id, label: i.name })),
          ]}
        />
        <Input
          label={t("inv.asset.tag")}
          value={form.tag}
          onChange={(e) => setForm({ ...form, tag: e.target.value })}
        />
        <Input
          label={t("inv.asset.serial")}
          value={form.serial_no}
          onChange={(e) => setForm({ ...form, serial_no: e.target.value })}
        />
        <Select
          label={t("inv.asset.status")}
          value={form.status}
          onChange={(e) => setForm({ ...form, status: e.target.value as AssetStatus })}
          options={STATUSES.map((s) => ({ value: s, label: t(`inv.asset.status.${s}`) }))}
        />
        <Select
          label={t("inv.location")}
          value={form.location_id ?? ""}
          onChange={(e) => setForm({ ...form, location_id: e.target.value || null })}
          options={[
            { value: "", label: t("inv.asset.noLocation") },
            ...(locations.data ?? []).map((l) => ({ value: l.id, label: l.name })),
          ]}
        />
        <Input
          label={t("inv.asset.purchasedOn")}
          type="date"
          value={form.purchased_on ?? ""}
          onChange={(e) => setForm({ ...form, purchased_on: e.target.value || null })}
        />
        <Input
          label={t("inv.asset.cost")}
          type="number"
          min={0}
          step="any"
          value={form.cost ?? ""}
          onChange={(e) =>
            setForm({ ...form, cost: e.target.value === "" ? null : e.target.value })
          }
        />
        <div>
          <Input
            label={t("inv.asset.assignedTo")}
            placeholder={t("inv.move.recipientSearch")}
            value={query || form.assigned_name}
            onChange={(e) => {
              setQuery(e.target.value);
              if (!e.target.value)
                setForm({ ...form, assigned_kind: null, assigned_id: null, assigned_name: "" });
            }}
          />
          {query && (matches.data ?? []).length > 0 && (
            <ul className="mt-1 max-h-32 overflow-auto rounded border border-border text-sm">
              {(matches.data ?? []).map((r) => (
                <li key={`${r.kind}-${r.id}`}>
                  <button
                    type="button"
                    className="flex w-full justify-between px-2 py-1 text-start hover:bg-selection"
                    onClick={() => pick(r)}
                  >
                    <span>{r.name}</span>
                    <span className="text-muted">{t(`inv.recipient.${r.kind}`)}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
      <Input
        label={t("inv.notes")}
        value={form.notes}
        onChange={(e) => setForm({ ...form, notes: e.target.value })}
        className="mt-3"
      />
    </Dialog>
  );
}
