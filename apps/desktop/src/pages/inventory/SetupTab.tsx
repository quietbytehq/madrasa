import { useState } from "react";
import { api } from "@/api/endpoints";
import type { InventoryCategory, InventoryLocation, LocationKind } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

const KINDS: LocationKind[] = ["store", "lab", "room", "office", "other"];

/** Categories and stock locations (inventory.manage). */
export function SetupTab() {
  const { t } = useI18n();
  const toast = useToast();
  const categories = useAsync(() => api.inventoryCategories(), []);
  const locations = useAsync(() => api.inventoryLocations(), []);
  const [category, setCategory] = useState("");
  const [location, setLocation] = useState({ name: "", kind: "store" as LocationKind });
  const [busy, setBusy] = useState(false);
  const run = async (f: () => Promise<unknown>, after: () => void) => {
    setBusy(true);
    try {
      await f();
      after();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <Card title={t("inv.setup.categories")}>
        <p className="mb-3 text-xs text-muted">{t("inv.setup.categoriesHint")}</p>
        <form
          className="mb-3 flex items-end gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!category.trim()) return;
            void run(
              () => api.createInventoryCategory({ name: category.trim() }),
              () => {
                setCategory("");
                void categories.reload();
              },
            );
          }}
        >
          <Input
            label={t("inv.setup.categoryName")}
            value={category}
            onChange={(e) => setCategory(e.target.value)}
            className="w-56"
          />
          <Button type="submit" size="sm" loading={busy} data-testid="add-category">
            {t("action.add")}
          </Button>
        </form>
        <Table<InventoryCategory>
          rows={categories.data ?? []}
          rowKey={(c) => c.id}
          pageSize={0}
          columns={[
            { key: "name", header: t("inv.setup.categoryName") },
            {
              key: "actions",
              header: "",
              render: (c) => (
                <div className="flex justify-end">
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={busy}
                    onClick={() =>
                      void run(
                        () => api.deleteInventoryCategory(c.id),
                        () => void categories.reload(),
                      )
                    }
                  >
                    {t("action.delete")}
                  </Button>
                </div>
              ),
            },
          ]}
        />
      </Card>
      <Card title={t("inv.setup.locations")}>
        <p className="mb-3 text-xs text-muted">{t("inv.setup.locationsHint")}</p>
        <form
          className="mb-3 flex flex-wrap items-end gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!location.name.trim()) return;
            void run(
              () =>
                api.createInventoryLocation({
                  name: location.name.trim(),
                  kind: location.kind,
                  active: true,
                  notes: "",
                }),
              () => {
                setLocation({ name: "", kind: "store" });
                void locations.reload();
              },
            );
          }}
        >
          <Input
            label={t("inv.setup.locationName")}
            value={location.name}
            onChange={(e) => setLocation({ ...location, name: e.target.value })}
            className="w-48"
          />
          <Select
            label={t("inv.setup.locationKind")}
            value={location.kind}
            onChange={(e) => setLocation({ ...location, kind: e.target.value as LocationKind })}
            options={KINDS.map((k) => ({ value: k, label: t(`inv.location.kind.${k}`) }))}
            className="w-32"
          />
          <Button type="submit" size="sm" loading={busy} data-testid="add-location">
            {t("action.add")}
          </Button>
        </form>
        <Table<InventoryLocation>
          rows={locations.data ?? []}
          rowKey={(l) => l.id}
          pageSize={0}
          columns={[
            { key: "name", header: t("inv.setup.locationName") },
            {
              key: "kind",
              header: t("inv.setup.locationKind"),
              render: (l) => t(`inv.location.kind.${l.kind}`),
            },
            {
              key: "actions",
              header: "",
              render: (l) => (
                <div className="flex justify-end">
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={busy}
                    onClick={() =>
                      void run(
                        () => api.deleteInventoryLocation(l.id),
                        () => void locations.reload(),
                      )
                    }
                  >
                    {t("action.delete")}
                  </Button>
                </div>
              ),
            },
          ]}
        />
      </Card>
    </div>
  );
}
