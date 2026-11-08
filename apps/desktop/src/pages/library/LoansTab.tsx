import { useState } from "react";
import { api } from "@/api/endpoints";
import type { LoanView } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useMoney } from "@/lib/useMoney";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { WaiveDialog } from "./DeskTab";
import { fineDue, isOverdue } from "./common";

const STATES = ["open", "overdue", "returned", "fines"] as const;

/** Loans by state: open, overdue, returned, fines still due. */
export function LoansTab({ manage }: { manage: boolean }) {
  const { t, formatDate } = useI18n();
  const money = useMoney();
  const toast = useToast();
  const [state, setState] = useState<(typeof STATES)[number]>("overdue");
  const loans = useAsync(() => api.libraryLoans({ state }), [state]);
  const [busy, setBusy] = useState(false);
  const [waiving, setWaiving] = useState<LoanView | null>(null);
  const act = async (f: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await f();
      await loans.reload();
      toast.push({ kind: "success", title: done });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      actions={
        <Select
          label={t("lib.loans.state")}
          value={state}
          onChange={(e) => setState(e.target.value as (typeof STATES)[number])}
          options={STATES.map((s) => ({ value: s, label: t(`lib.loans.state.${s}`) }))}
          className="w-40"
        />
      }
    >
      <p className="mb-3 text-xs text-muted">{t("lib.loans.hint")}</p>
      {loans.loading && !loans.data ? (
        <div className="flex justify-center py-6">
          <Spinner />
        </div>
      ) : (loans.data ?? []).length === 0 ? (
        <EmptyState title={t("lib.loans.empty")} />
      ) : (
        <Table<LoanView>
          rows={loans.data ?? []}
          rowKey={(l) => l.id}
          pageSize={25}
          filterable
          columns={[
            { key: "accession_no", header: t("lib.desk.accession") },
            { key: "title", header: t("lib.book.title"), sortable: true },
            {
              key: "member",
              header: t("lib.desk.member"),
              accessor: (l) => l.member_name,
              render: (l) => (
                <span>
                  <span className="font-medium">{l.member_name}</span>{" "}
                  <span className="text-xs text-muted">
                    {l.member_detail || t(`lib.member.${l.member_kind}`)}
                  </span>
                </span>
              ),
              sortable: true,
            },
            {
              key: "issued_on",
              header: t("lib.loans.issued"),
              render: (l) => formatDate(l.issued_on),
              sortable: true,
            },
            {
              key: "due_on",
              header: t("lib.desk.due"),
              render: (l) =>
                l.returned_on ? (
                  <span className="text-muted">
                    {t("lib.loans.returned")} {formatDate(l.returned_on)}
                  </span>
                ) : (
                  <span className={isOverdue(l) ? "font-medium text-danger" : ""}>
                    {formatDate(l.due_on)}
                    {isOverdue(l) && (
                      <Badge tone="danger" className="ms-1">
                        {t("lib.desk.overdue")}
                      </Badge>
                    )}
                  </span>
                ),
              sortable: true,
            },
            { key: "renewals", header: t("lib.loans.renewals"), align: "end" },
            {
              key: "fine",
              header: t("lib.fine.due"),
              align: "end",
              accessor: (l) => fineDue(l),
              render: (l) => (fineDue(l) > 0 ? money(fineDue(l)) : ""),
            },
            {
              key: "actions",
              header: "",
              render: (l) =>
                manage && (
                  <div className="flex justify-end gap-1">
                    {!l.returned_on && (
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy}
                        onClick={() =>
                          void act(() => api.returnLoan(l.id), t("lib.desk.returned", { fine: "" }))
                        }
                      >
                        {t("lib.desk.return")}
                      </Button>
                    )}
                    {fineDue(l) > 0 && (
                      <>
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={busy}
                          onClick={() => setWaiving(l)}
                        >
                          {t("lib.fine.waive")}
                        </Button>
                        <Button
                          size="sm"
                          disabled={busy}
                          onClick={() =>
                            void act(() => api.settleFine(l.id), t("lib.fine.settled"))
                          }
                        >
                          {t("lib.fine.settle")}
                        </Button>
                      </>
                    )}
                  </div>
                ),
            },
          ]}
        />
      )}
      {waiving && (
        <WaiveDialog
          loan={waiving}
          busy={busy}
          onClose={() => setWaiving(null)}
          onConfirm={(amount, reason) =>
            void act(() => api.waiveFine(waiving.id, amount, reason), t("lib.fine.waived")).then(
              () => setWaiving(null),
            )
          }
        />
      )}
    </Card>
  );
}
