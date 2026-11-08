import { useState } from "react";
import { api } from "@/api/endpoints";
import type { InventoryRecipient, MovementKind, MovementBody, ItemView } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

export const KINDS: MovementKind[] = [
  "purchase_in",
  "issue",
  "return",
  "write_off",
  "adjust_in",
  "adjust_out",
];

export function needsRecipient(kind: MovementKind): boolean {
  return kind === "issue" || kind === "return";
}

export function qty(v: string | number | null | undefined): string {
  if (v === null || v === undefined || v === "") return "";
  const n = Number(v);
  return Number.isFinite(n) ? String(n) : String(v);
}

export function today(): string {
  return new Date().toISOString().slice(0, 10);
}

/** Record one movement (or a transfer when `kind` is "transfer"). */
export function MovementDialog({
  item,
  kind: initialKind,
  onClose,
  onDone,
}: {
  item?: ItemView;
  kind?: MovementKind | "transfer";
  onClose: () => void;
  onDone: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const items = useAsync(
    () => (item ? Promise.resolve([item]) : api.inventoryItems({ active: true })),
    [item],
  );
  const locations = useAsync(() => api.inventoryLocations(), []);
  const [kind, setKind] = useState<MovementKind | "transfer">(initialKind ?? "purchase_in");
  const [itemId, setItemId] = useState(item?.id ?? "");
  const [location, setLocation] = useState("");
  const [toLocation, setToLocation] = useState("");
  const [quantity, setQuantity] = useState("1");
  const [unitCost, setUnitCost] = useState("");
  const [movedOn, setMovedOn] = useState(today());
  const [reference, setReference] = useState("");
  const [notes, setNotes] = useState("");
  const [query, setQuery] = useState("");
  const [recipient, setRecipient] = useState<InventoryRecipient | null>(null);
  const matches = useAsync(
    () =>
      query.trim().length >= 2 && !recipient
        ? api.inventoryRecipients({ q: query.trim() })
        : Promise.resolve([]),
    [query, recipient],
  );
  const [busy, setBusy] = useState(false);
  const loc = location || locations.data?.[0]?.id || "";
  const ready =
    itemId &&
    loc &&
    Number(quantity) > 0 &&
    (kind !== "transfer" || (toLocation && toLocation !== loc)) &&
    (kind === "transfer" || !needsRecipient(kind) || recipient);
  const save = async () => {
    if (!ready) return;
    setBusy(true);
    try {
      if (kind === "transfer") {
        await api.inventoryTransfer({
          item_id: itemId,
          from_location_id: loc,
          to_location_id: toLocation,
          quantity,
          moved_on: movedOn,
          notes,
        });
        toast.push({ kind: "success", title: t("inv.move.transferred") });
      } else {
        const body: MovementBody = {
          item_id: itemId,
          location_id: loc,
          kind,
          quantity,
          unit_cost: unitCost === "" ? null : unitCost,
          moved_on: movedOn,
          recipient_kind: recipient?.kind ?? null,
          recipient_id: recipient?.id ?? null,
          recipient_name: recipient?.name ?? "",
          reference,
          notes,
        };
        const r = await api.recordMovement(body);
        toast.push({
          kind: r.below_reorder ? "warning" : "success",
          title: t("inv.move.recorded", { n: qty(r.on_hand) }),
          description: r.below_reorder ? t("inv.move.belowReorder") : undefined,
        });
      }
      onDone();
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
      title={t("inv.move.title")}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!ready}
            onClick={() => void save()}
            data-testid="save-movement"
          >
            {t("action.save")}
          </Button>
        </div>
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Select
          label={t("inv.move.kind")}
          value={kind}
          onChange={(e) => {
            setKind(e.target.value as MovementKind | "transfer");
            setRecipient(null);
          }}
          options={[
            ...KINDS.map((k) => ({ value: k, label: t(`inv.kind.${k}`) })),
            { value: "transfer", label: t("inv.kind.transfer") },
          ]}
        />
        <Select
          label={t("inv.item")}
          value={itemId}
          onChange={(e) => setItemId(e.target.value)}
          disabled={!!item}
          options={[
            { value: "", label: t("inv.move.pickItem") },
            ...(items.data ?? []).map((i) => ({
              value: i.id,
              label: `${i.name} (${qty(i.on_hand)} ${i.unit})`,
            })),
          ]}
        />
        <Select
          label={kind === "transfer" ? t("inv.move.from") : t("inv.location")}
          value={loc}
          onChange={(e) => setLocation(e.target.value)}
          options={(locations.data ?? [])
            .filter((l) => l.active)
            .map((l) => ({ value: l.id, label: l.name }))}
        />
        {kind === "transfer" ? (
          <Select
            label={t("inv.move.to")}
            value={toLocation}
            onChange={(e) => setToLocation(e.target.value)}
            options={[
              { value: "", label: t("inv.move.pickLocation") },
              ...(locations.data ?? [])
                .filter((l) => l.active && l.id !== loc)
                .map((l) => ({ value: l.id, label: l.name })),
            ]}
          />
        ) : (
          <Input
            label={t("inv.move.date")}
            type="date"
            value={movedOn}
            onChange={(e) => setMovedOn(e.target.value)}
          />
        )}
        <Input
          label={t("inv.move.quantity")}
          type="number"
          min={0}
          step="any"
          value={quantity}
          onChange={(e) => setQuantity(e.target.value)}
        />
        {kind === "purchase_in" && (
          <Input
            label={t("inv.move.unitCost")}
            type="number"
            min={0}
            step="any"
            value={unitCost}
            onChange={(e) => setUnitCost(e.target.value)}
          />
        )}
        {kind === "transfer" && (
          <Input
            label={t("inv.move.date")}
            type="date"
            value={movedOn}
            onChange={(e) => setMovedOn(e.target.value)}
          />
        )}
      </div>
      {kind !== "transfer" && needsRecipient(kind) && (
        <div className="mt-3">
          <Input
            label={t(kind === "issue" ? "inv.move.issuedTo" : "inv.move.returnedBy")}
            placeholder={t("inv.move.recipientSearch")}
            value={
              recipient
                ? `${recipient.name}${recipient.detail ? ` · ${recipient.detail}` : ""}`
                : query
            }
            onChange={(e) => {
              setRecipient(null);
              setQuery(e.target.value);
            }}
          />
          {!recipient && (matches.data ?? []).length > 0 && (
            <ul
              className="mt-1 max-h-40 overflow-auto rounded border border-border text-sm"
              data-testid="recipients"
            >
              {(matches.data ?? []).map((r) => (
                <li key={`${r.kind}-${r.id}`}>
                  <button
                    type="button"
                    className="flex w-full justify-between px-2 py-1 text-start hover:bg-selection"
                    onClick={() => setRecipient(r)}
                  >
                    <span>{r.name}</span>
                    <span className="text-muted">
                      {t(`inv.recipient.${r.kind}`)}
                      {r.detail && ` · ${r.detail}`}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      <div className="mt-3 grid grid-cols-2 gap-3">
        {kind !== "transfer" && (
          <Input
            label={t("inv.move.reference")}
            value={reference}
            onChange={(e) => setReference(e.target.value)}
          />
        )}
        <Input label={t("inv.notes")} value={notes} onChange={(e) => setNotes(e.target.value)} />
      </div>
    </Dialog>
  );
}
