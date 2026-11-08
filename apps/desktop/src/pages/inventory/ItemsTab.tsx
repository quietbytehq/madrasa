import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { ItemBody, ItemDetail, ItemView } from "@/api/types";
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
import { errorMessage, useAsync } from "@/lib/useAsync";
import { MovementDialog, qty } from "./common";

const EMPTY: ItemBody = {
  category_id: null,
  name: "",
  code: "",
  unit: "pcs",
  reorder_level: null,
  is_asset: false,
  active: true,
  notes: "",
  custom: {},
};

/** Items with stock on hand and the reorder flag; a dialog per item. */
export function ItemsTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [below, setBelow] = useState(false);
  const categories = useAsync(() => api.inventoryCategories(), []);
  const [categoryId, setCategoryId] = useState("");
  const items = useAsync(
    () =>
      api.inventoryItems({
        q: query || undefined,
        category_id: categoryId || undefined,
        below_reorder: below || undefined,
      }),
    [query, categoryId, below],
  );
  const [open, setOpen] = useState<string | "new" | null>(null);
  const [moving, setMoving] = useState<ItemView | null>(null);
  return (
    <Card
      actions={
        manage && (
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="secondary"
              onClick={() => setMoving({} as ItemView)}
              data-testid="record-movement"
            >
              {t("inv.move.title")}
            </Button>
            <Button size="sm" onClick={() => setOpen("new")} data-testid="add-item">
              {t("inv.item.add")}
            </Button>
          </div>
        )
      }
    >
      <p className="mb-3 text-xs text-muted">{t("inv.item.hint")}</p>
      <div className="mb-3 flex flex-wrap items-end gap-2">
        <Input
          label={t("action.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-56"
        />
        <Select
          label={t("inv.category")}
          value={categoryId}
          onChange={(e) => setCategoryId(e.target.value)}
          options={[
            { value: "", label: t("inv.allCategories") },
            ...(categories.data ?? []).map((c) => ({ value: c.id, label: c.name })),
          ]}
          className="w-44"
        />
        <label className="flex items-center gap-2 pb-2 text-sm">
          <input type="checkbox" checked={below} onChange={(e) => setBelow(e.target.checked)} />
          {t("inv.item.belowOnly")}
        </label>
      </div>
      {items.loading && !items.data ? (
        <Spinner />
      ) : (items.data ?? []).length === 0 ? (
        <EmptyState title={t("inv.item.empty")} />
      ) : (
        <Table<ItemView>
          rows={items.data ?? []}
          rowKey={(i) => i.id}
          pageSize={50}
          onRowClick={(i) => setOpen(i.id)}
          columns={[
            { key: "name", header: t("inv.item"), sortable: true },
            { key: "code", header: t("inv.item.code") },
            {
              key: "category_name",
              header: t("inv.category"),
              sortable: true,
              render: (i) => i.category_name ?? "",
            },
            {
              key: "on_hand",
              header: t("inv.onHand"),
              align: "end",
              accessor: (i) => Number(i.on_hand),
              sortable: true,
              render: (i) => (
                <span>
                  {qty(i.on_hand)} <span className="text-xs text-muted">{i.unit}</span>{" "}
                  {i.below_reorder && <Badge tone="warning">{t("inv.item.reorder")}</Badge>}
                </span>
              ),
            },
            {
              key: "reorder_level",
              header: t("inv.item.reorderLevel"),
              align: "end",
              render: (i) => qty(i.reorder_level),
            },
            {
              key: "is_asset",
              header: t("inv.item.assets"),
              align: "end",
              render: (i) => (i.is_asset ? String(i.assets) : ""),
            },
          ]}
        />
      )}
      {open && (
        <ItemDialog
          id={open === "new" ? null : open}
          manage={manage}
          onClose={() => setOpen(null)}
          onChanged={() => void items.reload()}
        />
      )}
      {moving && (
        <MovementDialog
          item={moving.id ? moving : undefined}
          onClose={() => setMoving(null)}
          onDone={() => void items.reload()}
        />
      )}
    </Card>
  );
}

function ItemDialog({
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
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const categories = useAsync(() => api.inventoryCategories(), []);
  const detail = useAsync<ItemDetail | null>(
    () => (id ? api.inventoryItem(id) : Promise.resolve(null)),
    [id],
  );
  const [form, setForm] = useState<ItemBody>(EMPTY);
  const [busy, setBusy] = useState(false);
  const [moving, setMoving] = useState(false);
  useEffect(() => {
    const d = detail.data;
    if (d) {
      setForm({
        category_id: d.category_id,
        name: d.name,
        code: d.code,
        unit: d.unit,
        reorder_level: d.reorder_level,
        is_asset: d.is_asset,
        active: d.active,
        notes: d.notes,
        custom: d.custom,
      });
    }
  }, [detail.data]);
  const save = async () => {
    setBusy(true);
    try {
      const body: ItemBody = {
        ...form,
        reorder_level:
          form.reorder_level === null || form.reorder_level === "" ? null : form.reorder_level,
      };
      if (id) detail.setData(await api.updateInventoryItem(id, body));
      else await api.createInventoryItem(body);
      toast.push({ kind: "success", title: t("inv.item.saved") });
      onChanged();
      if (!id) onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!id || !window.confirm(t("inv.item.deleteConfirm"))) return;
    setBusy(true);
    try {
      await api.deleteInventoryItem(id);
      onChanged();
      onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const d = detail.data;
  if (id && !d) {
    return (
      <Dialog open onClose={onClose} title={t("inv.item.edit")}>
        <Spinner />
      </Dialog>
    );
  }
  return (
    <Dialog
      open
      onClose={onClose}
      size="lg"
      title={id ? t("inv.item.edit") : t("inv.item.add")}
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
              {id && d && (
                <Button variant="secondary" onClick={() => setMoving(true)}>
                  {t("inv.move.title")}
                </Button>
              )}
              <Button variant="secondary" onClick={onClose}>
                {t("action.cancel")}
              </Button>
              <Button loading={busy} onClick={() => void save()} data-testid="save-item">
                {t("action.save")}
              </Button>
            </span>
          </div>
        )
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Input
          label={t("inv.item.name")}
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          disabled={!manage}
        />
        <Input
          label={t("inv.item.code")}
          value={form.code}
          onChange={(e) => setForm({ ...form, code: e.target.value })}
          disabled={!manage}
        />
        <Select
          label={t("inv.category")}
          value={form.category_id ?? ""}
          onChange={(e) => setForm({ ...form, category_id: e.target.value || null })}
          disabled={!manage}
          options={[
            { value: "", label: t("inv.noCategory") },
            ...(categories.data ?? []).map((c) => ({ value: c.id, label: c.name })),
          ]}
        />
        <Input
          label={t("inv.item.unit")}
          value={form.unit}
          onChange={(e) => setForm({ ...form, unit: e.target.value })}
          disabled={!manage}
        />
        <Input
          label={t("inv.item.reorderLevel")}
          type="number"
          min={0}
          step="any"
          value={form.reorder_level ?? ""}
          onChange={(e) =>
            setForm({ ...form, reorder_level: e.target.value === "" ? null : e.target.value })
          }
          disabled={!manage}
        />
        <div className="flex flex-col justify-end gap-1 pb-1 text-sm">
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={form.is_asset}
              disabled={!manage}
              onChange={(e) => setForm({ ...form, is_asset: e.target.checked })}
            />
            {t("inv.item.isAsset")}
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={form.active}
              disabled={!manage}
              onChange={(e) => setForm({ ...form, active: e.target.checked })}
            />
            {t("inv.active")}
          </label>
        </div>
      </div>
      {d && (
        <>
          <div className="mb-1 mt-4 text-xs font-medium uppercase tracking-wide text-muted">
            {t("inv.onHand")}: {qty(d.on_hand)} {d.unit}
            {d.below_reorder && (
              <Badge tone="warning" className="ms-2">
                {t("inv.item.reorder")}
              </Badge>
            )}
          </div>
          {d.by_location.length > 0 && (
            <ul className="mb-3 text-sm">
              {d.by_location.map((s) => (
                <li
                  key={s.location_id}
                  className="flex justify-between border-b border-border/50 py-1"
                >
                  <span>{s.location_name}</span>
                  <span>
                    {qty(s.on_hand)} {s.unit}
                  </span>
                </li>
              ))}
            </ul>
          )}
          <div className="mb-1 text-xs font-medium uppercase tracking-wide text-muted">
            {t("inv.item.recent")}
          </div>
          {d.recent.length === 0 ? (
            <p className="text-sm text-muted">{t("inv.move.empty")}</p>
          ) : (
            <table className="w-full text-sm">
              <tbody>
                {d.recent.map((m) => (
                  <tr key={m.id} className="border-b border-border/50">
                    <td className="py-1 pe-2 text-muted">{formatDate(m.moved_on)}</td>
                    <td className="py-1 pe-2">{t(`inv.kind.${m.kind}`)}</td>
                    <td className="py-1 pe-2 text-end">{qty(m.quantity)}</td>
                    <td className="py-1 pe-2 text-muted">{m.location_name}</td>
                    <td className="py-1 text-muted">{m.recipient_name || m.reference}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {d.asset_list.length > 0 && (
            <>
              <div className="mb-1 mt-3 text-xs font-medium uppercase tracking-wide text-muted">
                {t("inv.tab.assets")}
              </div>
              <ul className="text-sm">
                {d.asset_list.map((a) => (
                  <li key={a.id} className="flex justify-between border-b border-border/50 py-1">
                    <span>
                      <span className="font-medium">{a.tag}</span>{" "}
                      <span className="text-muted">{a.serial_no}</span>
                    </span>
                    <span className="text-muted">
                      {t(`inv.asset.status.${a.status}`)}
                      {a.assigned_name && ` · ${a.assigned_name}`}
                      {a.location_name && ` · ${a.location_name}`}
                    </span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </>
      )}
      {moving && d && (
        <MovementDialog
          item={d}
          onClose={() => setMoving(false)}
          onDone={() => {
            void detail.reload();
            onChanged();
          }}
        />
      )}
    </Dialog>
  );
}
