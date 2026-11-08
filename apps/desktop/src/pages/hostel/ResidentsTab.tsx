import { useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import type { AllocationView } from "@/api/types";
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
import { today } from "./common";

/** Every resident of the year with hostel, room, bed and monthly charge. */
export function ResidentsTab({ manage }: { manage: boolean }) {
  const { t, formatDate } = useI18n();
  const money = useMoney();
  const toast = useToast();
  const years = useAsync(() => api.academicYears(), []);
  const active = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const [yearId, setYearId] = useState("");
  const year = yearId || active?.id || "";
  const hostels = useAsync(() => api.hostels(), []);
  const [hostelId, setHostelId] = useState("");
  const [query, setQuery] = useState("");
  const residents = useAsync(
    () =>
      year
        ? api.hostelResidents({
            academic_year_id: year,
            hostel_id: hostelId || undefined,
            q: query || undefined,
          })
        : Promise.resolve([]),
    [year, hostelId, query],
  );
  const [busy, setBusy] = useState(false);
  const end = async (a: AllocationView) => {
    if (!window.confirm(t("hst.allocate.endConfirm", { name: a.student_name }))) return;
    setBusy(true);
    try {
      await api.updateAllocation(a.id, { to_date: today() });
      toast.push({ kind: "success", title: t("hst.allocate.ended") });
      await residents.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      actions={
        <div className="flex flex-wrap items-end gap-2">
          <Select
            label={t("tt.year")}
            value={year}
            onChange={(e) => setYearId(e.target.value)}
            options={(years.data ?? []).map((y) => ({ value: y.id, label: y.name }))}
            className="w-36"
          />
          <Select
            label={t("hst.hostel")}
            value={hostelId}
            onChange={(e) => setHostelId(e.target.value)}
            options={[
              { value: "", label: t("hst.allHostels") },
              ...(hostels.data ?? []).map((h) => ({ value: h.id, label: h.name })),
            ]}
            className="w-44"
          />
          <Input
            label={t("action.search")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="w-48"
          />
        </div>
      }
    >
      {residents.loading && !residents.data ? (
        <Spinner />
      ) : (residents.data ?? []).length === 0 ? (
        <EmptyState title={t("hst.residents.empty")} />
      ) : (
        <Table<AllocationView>
          rows={residents.data ?? []}
          rowKey={(a) => a.id}
          pageSize={50}
          columns={[
            { key: "student_number", header: t("students.number") },
            { key: "student_name", header: t("students.fullName"), sortable: true },
            {
              key: "class_name",
              header: t("hst.class"),
              render: (a) => `${a.class_name}-${a.section_name}`,
            },
            { key: "hostel_name", header: t("hst.hostel"), sortable: true },
            {
              key: "room_name",
              header: t("hst.room"),
              render: (a) => (
                <span>
                  {a.room_name}{" "}
                  <span className="text-xs text-muted">
                    {t("hst.bed")} {a.bed_no}
                  </span>
                </span>
              ),
            },
            {
              key: "from_date",
              header: t("hst.allocate.from"),
              render: (a) => formatDate(a.from_date),
            },
            {
              key: "charge",
              header: t("hst.charge"),
              align: "end",
              render: (a) => money(a.charge),
            },
            {
              key: "actions",
              header: "",
              render: (a) =>
                manage && (
                  <div className="flex justify-end">
                    <Button size="sm" variant="ghost" disabled={busy} onClick={() => void end(a)}>
                      {t("hst.allocate.end")}
                    </Button>
                  </div>
                ),
            },
          ]}
        />
      )}
    </Card>
  );
}
