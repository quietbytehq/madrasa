import { useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import type { LeaveBalance } from "@/api/types";
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
import { days } from "./common";

/** Balances per staff member and leave type for a year; adjustments and carry-over. */
export function BalancesTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const toast = useToast();
  const years = useAsync(() => api.academicYears(), []);
  const active = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const [yearId, setYearId] = useState("");
  const year = yearId || active?.id || "";
  const list = useAsync(
    () => (year ? api.leaveBalances({ academic_year_id: year }) : Promise.resolve([])),
    [year],
  );
  const [editing, setEditing] = useState<LeaveBalance | null>(null);
  const [carryTo, setCarryTo] = useState("");
  const [busy, setBusy] = useState(false);
  const act = async (f: () => Promise<unknown>, done: (r: unknown) => string) => {
    setBusy(true);
    try {
      const r = await f();
      await list.reload();
      toast.push({ kind: "success", title: done(r) });
      setEditing(null);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const later = (years.data ?? []).filter((y) => active && y.start_date > active.start_date);
  return (
    <Card
      actions={
        <div className="flex flex-wrap items-end gap-2">
          <Select
            label={t("tt.year")}
            value={year}
            onChange={(e) => setYearId(e.target.value)}
            options={(years.data ?? []).map((y) => ({ value: y.id, label: y.name }))}
            className="w-40"
          />
          {manage && later.length > 0 && (
            <>
              <Select
                label={t("leave.balance.carryOver")}
                value={carryTo}
                onChange={(e) => setCarryTo(e.target.value)}
                options={[
                  { value: "", label: t("leave.balance.carryOverPick") },
                  ...later.map((y) => ({ value: y.id, label: y.name })),
                ]}
                className="w-44"
              />
              <Button
                size="sm"
                variant="secondary"
                disabled={!carryTo || busy}
                onClick={() =>
                  void act(
                    () => api.leaveCarryOver(year, carryTo),
                    (r) =>
                      t("leave.balance.carryOverDone", { n: (r as { written: number }).written }),
                  )
                }
              >
                {t("leave.balance.carryOverRun")}
              </Button>
            </>
          )}
        </div>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("leave.balances.hint")}</p>
      {list.loading && !list.data ? (
        <div className="flex justify-center py-6">
          <Spinner />
        </div>
      ) : !list.data || list.data.length === 0 ? (
        <EmptyState title={t("leave.balances.empty")} />
      ) : (
        <Table<LeaveBalance>
          rows={list.data}
          rowKey={(b) => `${b.staff_id}-${b.leave_type_id}`}
          filterable
          pageSize={50}
          onRowClick={manage ? setEditing : undefined}
          columns={[
            { key: "staff_name", header: t("leave.request.staff"), sortable: true },
            { key: "type_name", header: t("leave.request.type"), sortable: true },
            {
              key: "entitlement",
              header: t("leave.balance.entitlement"),
              align: "end",
              render: (b) => days(b.entitlement),
            },
            {
              key: "carried_over",
              header: t("leave.balance.carried"),
              align: "end",
              render: (b) => days(b.carried_over),
            },
            {
              key: "adjustment",
              header: t("leave.balance.adjustment"),
              align: "end",
              render: (b) => days(b.adjustment),
            },
            {
              key: "used",
              header: t("leave.balance.used"),
              align: "end",
              render: (b) => days(b.used),
            },
            {
              key: "pending",
              header: t("leave.balance.pending"),
              align: "end",
              render: (b) => days(b.pending),
            },
            {
              key: "available",
              header: t("leave.balance.available"),
              align: "end",
              render: (b) => (
                <span
                  className={Number(b.available) < 0 ? "font-medium text-danger" : "font-medium"}
                >
                  {days(b.available)}
                </span>
              ),
            },
          ]}
        />
      )}
      {editing && (
        <BalanceDialog
          balance={editing}
          busy={busy}
          onClose={() => setEditing(null)}
          onSave={(entitlement, carried, adjustment, note) =>
            void act(
              () =>
                api.setLeaveBalance({
                  academic_year_id: year,
                  staff_id: editing.staff_id,
                  leave_type_id: editing.leave_type_id,
                  entitlement,
                  carried_over: carried,
                  adjustment,
                  note,
                }),
              () => t("leave.balance.saved"),
            )
          }
        />
      )}
    </Card>
  );
}

function BalanceDialog({
  balance,
  busy,
  onClose,
  onSave,
}: {
  balance: LeaveBalance;
  busy: boolean;
  onClose: () => void;
  onSave: (entitlement: number | null, carried: number, adjustment: number, note: string) => void;
}) {
  const { t } = useI18n();
  const [entitlement, setEntitlement] = useState(balance.id ? days(balance.entitlement) : "");
  const [carried, setCarried] = useState(days(balance.carried_over));
  const [adjustment, setAdjustment] = useState(days(balance.adjustment));
  const [note, setNote] = useState(balance.note);
  return (
    <Dialog
      open
      onClose={onClose}
      title={`${t("leave.balance.edit")}: ${balance.staff_name} · ${balance.type_name}`}
      footer={
        <div className="flex gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            onClick={() =>
              onSave(
                entitlement === "" ? null : Number(entitlement),
                Number(carried) || 0,
                Number(adjustment) || 0,
                note,
              )
            }
          >
            {t("action.save")}
          </Button>
        </div>
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Input
          label={t("leave.balance.entitlementOverride")}
          type="number"
          step="0.5"
          value={entitlement}
          onChange={(e) => setEntitlement(e.target.value)}
        />
        <Input
          label={t("leave.balance.carried")}
          type="number"
          step="0.5"
          value={carried}
          onChange={(e) => setCarried(e.target.value)}
        />
        <Input
          label={t("leave.balance.adjustment")}
          type="number"
          step="0.5"
          value={adjustment}
          onChange={(e) => setAdjustment(e.target.value)}
        />
        <Input
          label={t("leave.balance.note")}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </div>
    </Dialog>
  );
}
