import { useState } from "react";
import { api } from "@/api/endpoints";
import type { CopyStatus, CopyView, StockRow } from "@/api/types";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { CopyStatusBadge } from "./common";

const STATUSES: CopyStatus[] = ["available", "on_loan", "lost", "withdrawn"];

/** Stock take: counts by category and every copy with its status. */
export function StockTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const toast = useToast();
  const stock = useAsync(() => api.libraryStock(), []);
  const [status, setStatus] = useState("");
  const copies = useAsync(() => api.libraryCopies(status || undefined), [status]);
  const [busy, setBusy] = useState(false);
  const setCopyStatus = async (c: CopyView, next: CopyStatus) => {
    setBusy(true);
    try {
      await api.updateCopy(c.id, { shelf: c.shelf, condition: c.condition, status: next });
      await Promise.all([copies.reload(), stock.reload()]);
      toast.push({ kind: "success", title: t("lib.copy.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-4">
      <Card title={t("lib.tab.stock")}>
        <p className="mb-3 text-xs text-muted">{t("lib.stock.hint")}</p>
        {stock.loading && !stock.data ? (
          <Spinner />
        ) : (
          <Table<StockRow>
            rows={stock.data ?? []}
            rowKey={(r) => r.category || "-"}
            pageSize={0}
            columns={[
              { key: "category", header: t("lib.book.category"), render: (r) => r.category || "—" },
              { key: "titles", header: t("lib.stock.titles"), align: "end" },
              { key: "copies", header: t("lib.book.copies"), align: "end" },
              { key: "available", header: t("lib.copy.status.available"), align: "end" },
              { key: "on_loan", header: t("lib.copy.status.on_loan"), align: "end" },
              { key: "lost", header: t("lib.copy.status.lost"), align: "end" },
              { key: "withdrawn", header: t("lib.copy.status.withdrawn"), align: "end" },
            ]}
          />
        )}
      </Card>
      <Card
        actions={
          <Select
            label={t("lib.stock.filter")}
            value={status}
            onChange={(e) => setStatus(e.target.value)}
            options={[
              { value: "", label: t("lib.stock.any") },
              ...STATUSES.map((s) => ({ value: s, label: t(`lib.copy.status.${s}`) })),
            ]}
            className="w-40"
          />
        }
      >
        {copies.loading && !copies.data ? (
          <Spinner />
        ) : (copies.data ?? []).length === 0 ? (
          <EmptyState title={t("lib.loans.empty")} />
        ) : (
          <Table<CopyView>
            rows={copies.data ?? []}
            rowKey={(c) => c.id}
            pageSize={50}
            filterable
            columns={[
              { key: "accession_no", header: t("lib.desk.accession"), sortable: true },
              { key: "title", header: t("lib.book.title"), sortable: true },
              { key: "category", header: t("lib.book.category"), sortable: true },
              { key: "shelf", header: t("lib.copies.shelf") },
              {
                key: "condition",
                header: t("lib.copy.condition"),
                render: (c) => t(`lib.copy.condition.${c.condition}`),
              },
              {
                key: "status",
                header: t("lib.desk.status"),
                render: (c) =>
                  manage && c.status !== "on_loan" ? (
                    <Select
                      aria-label={`${c.accession_no}: ${t("lib.desk.status")}`}
                      value={c.status}
                      disabled={busy}
                      onChange={(e) => void setCopyStatus(c, e.target.value as CopyStatus)}
                      options={(["available", "lost", "withdrawn"] as CopyStatus[]).map((s) => ({
                        value: s,
                        label: t(`lib.copy.status.${s}`),
                      }))}
                      className="w-32"
                    />
                  ) : (
                    <CopyStatusBadge status={c.status} />
                  ),
              },
            ]}
          />
        )}
      </Card>
    </div>
  );
}
