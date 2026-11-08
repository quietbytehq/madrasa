import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { HostelBody, HostelDetail, HostelView, HostelRoomBody, HostelRoom } from "@/api/types";
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

const EMPTY: HostelBody = {
  name: "",
  kind: "mixed",
  warden_name: "",
  warden_phone: "",
  monthly_fee: 0,
  fee_head_id: null,
  requires_consent: true,
  active: true,
  notes: "",
  custom: {},
};
const EMPTY_ROOM: HostelRoomBody = {
  floor: "",
  name: "",
  beds: 2,
  monthly_fee: null,
  active: true,
  notes: "",
};

/** Hostels with their rooms, fee and fee head. */
export function HostelsTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const money = useMoney();
  const hostels = useAsync(() => api.hostels(), []);
  const [open, setOpen] = useState<string | "new" | null>(null);
  return (
    <Card
      actions={
        manage && (
          <Button size="sm" onClick={() => setOpen("new")} data-testid="add-hostel">
            {t("hst.hostel.add")}
          </Button>
        )
      }
    >
      <p className="mb-3 text-xs text-muted">{t("hst.hostel.hint")}</p>
      {hostels.loading && !hostels.data ? (
        <Spinner />
      ) : (hostels.data ?? []).length === 0 ? (
        <EmptyState title={t("hst.hostel.empty")} />
      ) : (
        <Table<HostelView>
          rows={hostels.data ?? []}
          rowKey={(h) => h.id}
          pageSize={0}
          onRowClick={(h) => setOpen(h.id)}
          columns={[
            { key: "name", header: t("hst.hostel.name"), sortable: true },
            {
              key: "kind",
              header: t("hst.hostel.kind"),
              render: (h) => t(`hst.hostel.kind.${h.kind}`),
            },
            { key: "warden_name", header: t("hst.hostel.warden") },
            { key: "rooms", header: t("hst.rooms"), align: "end" },
            {
              key: "occupied",
              header: t("hst.residents"),
              align: "end",
              render: (h) => (
                <Badge tone={h.occupied >= h.beds && h.beds > 0 ? "warning" : "neutral"}>
                  {h.occupied} / {h.beds}
                </Badge>
              ),
            },
            {
              key: "monthly_fee",
              header: t("hst.hostel.fee"),
              align: "end",
              render: (h) => money(h.monthly_fee),
            },
            {
              key: "fee_head_name",
              header: t("hst.hostel.feeHead"),
              render: (h) => h.fee_head_name ?? "—",
            },
          ]}
        />
      )}
      {open && (
        <HostelDialog
          id={open === "new" ? null : open}
          manage={manage}
          onClose={() => setOpen(null)}
          onChanged={() => void hostels.reload()}
        />
      )}
    </Card>
  );
}

function HostelDialog({
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
  const heads = useAsync(() => api.feeHeads(), []);
  const detail = useAsync<HostelDetail | null>(
    () => (id ? api.hostel(id) : Promise.resolve(null)),
    [id],
  );
  const [form, setForm] = useState<HostelBody>(EMPTY);
  const [room, setRoom] = useState<HostelRoomBody>(EMPTY_ROOM);
  const [editingRoom, setEditingRoom] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const d = detail.data;
    if (d) {
      setForm({
        name: d.name,
        kind: d.kind,
        warden_name: d.warden_name,
        warden_phone: d.warden_phone,
        monthly_fee: d.monthly_fee,
        fee_head_id: d.fee_head_id,
        requires_consent: d.requires_consent,
        active: d.active,
        notes: d.notes,
        custom: d.custom,
      });
    }
  }, [detail.data]);
  const run = async (f: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await f();
      toast.push({ kind: "success", title: done });
      onChanged();
      return true;
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
      return false;
    } finally {
      setBusy(false);
    }
  };
  const save = async () => {
    const body = { ...form, monthly_fee: Number(form.monthly_fee) || 0 };
    const ok = await run(
      () => (id ? api.updateHostel(id, body) : api.createHostel(body)),
      t("hst.hostel.saved"),
    );
    if (ok) onClose();
  };
  const remove = async () => {
    if (!id || !window.confirm(t("hst.hostel.deleteConfirm"))) return;
    if (await run(() => api.deleteHostel(id), t("hst.hostel.deleted"))) onClose();
  };
  const saveRoom = async () => {
    if (!id) return;
    const body: HostelRoomBody = {
      ...room,
      beds: Number(room.beds) || 0,
      monthly_fee:
        room.monthly_fee === null || room.monthly_fee === "" ? null : Number(room.monthly_fee),
    };
    const ok = await run(
      () =>
        editingRoom ? api.updateHostelRoom(editingRoom, body) : api.createHostelRoom(id, body),
      t("hst.room.saved"),
    );
    if (ok) {
      setRoom(EMPTY_ROOM);
      setEditingRoom(null);
      await detail.reload();
    }
  };
  const removeRoom = async (r: HostelRoom) => {
    if (!window.confirm(t("hst.room.deleteConfirm", { name: r.name }))) return;
    if (await run(() => api.deleteHostelRoom(r.id), t("hst.room.deleted"))) await detail.reload();
  };
  if (id && !detail.data) {
    return (
      <Dialog open onClose={onClose} title={t("hst.hostel.edit")}>
        <Spinner />
      </Dialog>
    );
  }
  return (
    <Dialog
      open
      onClose={onClose}
      size="lg"
      title={id ? t("hst.hostel.edit") : t("hst.hostel.add")}
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
              <Button loading={busy} onClick={() => void save()} data-testid="save-hostel">
                {t("action.save")}
              </Button>
            </span>
          </div>
        )
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Input
          label={t("hst.hostel.name")}
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          disabled={!manage}
        />
        <Select
          label={t("hst.hostel.kind")}
          value={form.kind}
          onChange={(e) => setForm({ ...form, kind: e.target.value as HostelBody["kind"] })}
          disabled={!manage}
          options={(["boys", "girls", "mixed"] as const).map((k) => ({
            value: k,
            label: t(`hst.hostel.kind.${k}`),
          }))}
        />
        <Input
          label={t("hst.hostel.warden")}
          value={form.warden_name}
          onChange={(e) => setForm({ ...form, warden_name: e.target.value })}
          disabled={!manage}
        />
        <Input
          label={t("hst.hostel.wardenPhone")}
          value={form.warden_phone}
          onChange={(e) => setForm({ ...form, warden_phone: e.target.value })}
          disabled={!manage}
        />
        <Input
          label={t("hst.hostel.fee")}
          type="number"
          min={0}
          value={String(form.monthly_fee)}
          onChange={(e) => setForm({ ...form, monthly_fee: e.target.value })}
          disabled={!manage}
        />
        <Select
          label={t("hst.hostel.feeHead")}
          value={form.fee_head_id ?? ""}
          onChange={(e) => setForm({ ...form, fee_head_id: e.target.value || null })}
          disabled={!manage}
          options={[
            { value: "", label: t("hst.hostel.noFeeHead") },
            ...(heads.data ?? []).map((h) => ({ value: h.id, label: h.name })),
          ]}
        />
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={form.requires_consent}
            disabled={!manage}
            onChange={(e) => setForm({ ...form, requires_consent: e.target.checked })}
          />
          {t("hst.hostel.requiresConsent")}
        </label>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={form.active}
            disabled={!manage}
            onChange={(e) => setForm({ ...form, active: e.target.checked })}
          />
          {t("hst.active")}
        </label>
      </div>
      {id && (
        <>
          <div className="mb-1 mt-4 text-xs font-medium uppercase tracking-wide text-muted">
            {t("hst.rooms")}
          </div>
          <p className="mb-2 text-xs text-muted">{t("hst.room.hint")}</p>
          {(detail.data?.room_list ?? []).length === 0 ? (
            <p className="mb-2 text-sm text-muted">{t("hst.room.empty")}</p>
          ) : (
            <Table<HostelRoom>
              rows={detail.data?.room_list ?? []}
              rowKey={(r) => r.id}
              pageSize={0}
              columns={[
                { key: "floor", header: t("hst.room.floor") },
                { key: "name", header: t("hst.room") },
                { key: "beds", header: t("hst.room.beds"), align: "end" },
                { key: "occupied", header: t("hst.residents"), align: "end" },
                {
                  key: "monthly_fee",
                  header: t("hst.room.fee"),
                  align: "end",
                  render: (r) => r.monthly_fee ?? "",
                },
                {
                  key: "actions",
                  header: "",
                  render: (r) =>
                    manage && (
                      <div className="flex justify-end gap-1">
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => {
                            setEditingRoom(r.id);
                            setRoom({
                              floor: r.floor,
                              name: r.name,
                              beds: r.beds,
                              monthly_fee: r.monthly_fee,
                              active: r.active,
                              notes: r.notes,
                            });
                          }}
                        >
                          {t("action.edit")}
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={busy}
                          onClick={() => void removeRoom(r)}
                        >
                          {t("action.delete")}
                        </Button>
                      </div>
                    ),
                },
              ]}
            />
          )}
          {manage && (
            <div
              className="mt-2 flex flex-wrap items-end gap-2 rounded border border-border p-2"
              data-testid="room-form"
            >
              <Input
                label={t("hst.room.floor")}
                value={room.floor}
                onChange={(e) => setRoom({ ...room, floor: e.target.value })}
                className="w-28"
              />
              <Input
                label={t("hst.room")}
                value={room.name}
                onChange={(e) => setRoom({ ...room, name: e.target.value })}
                className="w-28"
              />
              <Input
                label={t("hst.room.beds")}
                type="number"
                min={0}
                value={String(room.beds)}
                onChange={(e) => setRoom({ ...room, beds: e.target.value })}
                className="w-20"
              />
              <Input
                label={t("hst.room.fee")}
                type="number"
                min={0}
                placeholder={t("hst.allocate.defaultFee")}
                value={room.monthly_fee === null ? "" : String(room.monthly_fee)}
                onChange={(e) =>
                  setRoom({ ...room, monthly_fee: e.target.value === "" ? null : e.target.value })
                }
                className="w-28"
              />
              <Button
                size="sm"
                loading={busy}
                onClick={() => void saveRoom()}
                data-testid="save-room"
              >
                {editingRoom ? t("action.save") : t("hst.room.add")}
              </Button>
              {editingRoom && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setEditingRoom(null);
                    setRoom(EMPTY_ROOM);
                  }}
                >
                  {t("action.cancel")}
                </Button>
              )}
            </div>
          )}
        </>
      )}
    </Dialog>
  );
}
