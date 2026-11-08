import { useState } from "react";
import { api } from "@/api/endpoints";
import type { InventoryStockRow } from "@/api/types";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { qty } from "./common";

/** Stock on hand per item and location. */
export function StockTab() {
  const { t } = useI18n();
  const locations = useAsync(() => api.inventoryLocations(), []);
  const [locationId, setLocationId] = useState("");
  const stock = useAsync(
    () => api.inventoryStock({ location_id: locationId || undefined }),
    [locationId],
  );
  return (
    <Card
      actions={
        <Select
          label={t("inv.location")}
          value={locationId}
          onChange={(e) => setLocationId(e.target.value)}
          options={[
            { value: "", label: t("inv.allLocations") },
            ...(locations.data ?? []).map((l) => ({ value: l.id, label: l.name })),
          ]}
          className="w-44"
        />
      }
    >
      <p className="mb-3 text-xs text-muted">{t("inv.stock.hint")}</p>
      {stock.loading && !stock.data ? (
        <Spinner />
      ) : (stock.data ?? []).length === 0 ? (
        <EmptyState title={t("inv.stock.empty")} />
      ) : (
        <Table<InventoryStockRow>
          rows={stock.data ?? []}
          rowKey={(r) => `${r.item_id}-${r.location_id}`}
          pageSize={100}
          filterable
          columns={[
            {
              key: "category_name",
              header: t("inv.category"),
              sortable: true,
              render: (r) => r.category_name ?? "",
            },
            { key: "item_name", header: t("inv.item"), sortable: true },
            { key: "code", header: t("inv.item.code") },
            { key: "location_name", header: t("inv.location"), sortable: true },
            {
              key: "on_hand",
              header: t("inv.onHand"),
              align: "end",
              accessor: (r) => Number(r.on_hand),
              sortable: true,
              render: (r) => `${qty(r.on_hand)} ${r.unit}`,
            },
          ]}
        />
      )}
    </Card>
  );
}
