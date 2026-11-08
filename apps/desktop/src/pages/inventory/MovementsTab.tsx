import { useState } from "react";
import { api } from "@/api/endpoints";
import type { MovementView } from "@/api/types";
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
import { KINDS, MovementDialog, qty } from "./common";

const IN = new Set(["purchase_in", "return", "adjust_in", "transfer_in"]);

/** The movement register with filters; record a movement or remove a wrong one. */
export function MovementsTab({ manage }: { manage: boolean }) {
  const { t, formatDate } = useI18n();
  const money = useMoney();
  const toast = useToast();
  const locations = useAsync(() => api.inventoryLocations(), []);
  const [kind, setKind] = useState("");
  const [locationId, setLocationId] = useState("");
  const [query, setQuery] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const movements = useAsync(
    () =>
      api.inventoryMovements({
        kind: kind || undefined,
        location_id: locationId || undefined,
        q: query || undefined,
        from: from || undefined,
        to: to || undefined,
      }),
    [kind, locationId, query, from, to],
  );
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const remove = async (m: MovementView) => {
    if (!window.confirm(t("inv.move.deleteConfirm"))) return;
    setBusy(true);
    try {
      await api.deleteMovement(m.id);
      await movements.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      actions={
        manage && (
          <Button size="sm" onClick={() => setAdding(true)} data-testid="record-movement">
            {t("inv.move.title")}
          </Button>
        )
      }
    >
      <p className="mb-3 text-xs text-muted">{t("inv.move.hint")}</p>
      <div className="mb-3 flex flex-wrap items-end gap-2">
        <Select
          label={t("inv.move.kind")}
          value={kind}
          onChange={(e) => setKind(e.target.value)}
          options={[
            { value: "", label: t("inv.move.anyKind") },
            ...KINDS.map((k) => ({ value: k, label: t(`inv.kind.${k}`) })),
            { value: "transfer_out", label: t("inv.kind.transfer_out") },
            { value: "transfer_in", label: t("inv.kind.transfer_in") },
          ]}
          className="w-40"
        />
        <Select
          label={t("inv.location")}
          value={locationId}
          onChange={(e) => setLocationId(e.target.value)}
          options={[
            { value: "", label: t("inv.allLocations") },
            ...(locations.data ?? []).map((l) => ({ value: l.id, label: l.name })),
          ]}
          className="w-40"
        />
        <Input
          label={t("inv.move.from")}
          type="date"
          value={from}
          onChange={(e) => setFrom(e.target.value)}
          className="w-40"
        />
        <Input
          label={t("inv.move.to")}
          type="date"
          value={to}
          onChange={(e) => setTo(e.target.value)}
          className="w-40"
        />
        <Input
          label={t("action.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-48"
        />
      </div>
      {movements.loading && !movements.data ? (
        <Spinner />
      ) : (movements.data ?? []).length === 0 ? (
        <EmptyState title={t("inv.move.empty")} />
      ) : (
        <Table<MovementView>
          rows={movements.data ?? []}
          rowKey={(m) => m.id}
          pageSize={50}
          columns={[
            {
              key: "moved_on",
              header: t("inv.move.date"),
              render: (m) => formatDate(m.moved_on),
              sortable: true,
            },
            {
              key: "kind",
              header: t("inv.move.kind"),
              render: (m) => (
                <Badge tone={IN.has(m.kind) ? "success" : "neutral"}>
                  {t(`inv.kind.${m.kind}`)}
                </Badge>
              ),
            },
            { key: "item_name", header: t("inv.item"), sortable: true },
            {
              key: "quantity",
              header: t("inv.move.quantity"),
              align: "end",
              render: (m) => (
                <span>
                  {IN.has(m.kind) ? "+" : "−"}
                  {qty(m.quantity)} <span className="text-xs text-muted">{m.unit}</span>
                </span>
              ),
            },
            { key: "location_name", header: t("inv.location") },
            {
              key: "recipient_name",
              header: t("inv.move.recipient"),
              render: (m) => (
                <span>
                  {m.recipient_name}
                  {m.recipient_kind && (
                    <span className="text-xs text-muted">
                      {" "}
                      · {t(`inv.recipient.${m.recipient_kind}`)}
                    </span>
                  )}
                </span>
              ),
            },
            { key: "reference", header: t("inv.move.reference") },
            {
              key: "total_cost",
              header: t("inv.move.cost"),
              align: "end",
              render: (m) => (m.total_cost ? money(m.total_cost) : ""),
            },
            {
              key: "actions",
              header: "",
              render: (m) =>
                manage && (
                  <div className="flex justify-end">
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => void remove(m)}
                    >
                      {t("action.delete")}
                    </Button>
                  </div>
                ),
            },
          ]}
        />
      )}
      {adding && (
        <MovementDialog onClose={() => setAdding(false)} onDone={() => void movements.reload()} />
      )}
    </Card>
  );
}
