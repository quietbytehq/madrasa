import { todayIso } from "@/lib/dates";
import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "@/api/endpoints";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { Amount, methodLabel } from "../common";

export function CollectionsTab() {
  const { t, formatDate } = useI18n();
  const [date, setDate] = useState(todayIso());
  const data = useAsync(() => api.collections(date), [date]);
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-end gap-2 print:hidden">
        <Input
          label={t("fees.collections.date")}
          type="date"
          value={date}
          onChange={(e) => setDate(e.target.value)}
          className="w-44"
        />
        <span className="flex-1" />
        <Button variant="secondary" onClick={() => window.print()}>
          {t("action.print")}
        </Button>
      </div>
      {data.loading && !data.data ? (
        <div className="flex justify-center py-10">
          <Spinner />
        </div>
      ) : data.data ? (
        <div className="print-area flex flex-col gap-3">
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            <Card padded={false} className="p-3">
              <div className="text-2xs uppercase tracking-wide text-muted">
                {t("fees.collections.total")} · {formatDate(data.data.date)}
              </div>
              <div className="mt-1 text-lg font-semibold tabular-nums text-navy">
                <Amount value={data.data.total} />
              </div>
            </Card>
            {data.data.by_method.map((m) => (
              <Card key={m.method} padded={false} className="p-3">
                <div className="text-2xs uppercase tracking-wide text-muted">
                  {methodLabel(t, m.method)} · {m.receipts}
                </div>
                <div className="mt-1 text-lg font-semibold tabular-nums text-navy">
                  <Amount value={m.amount} />
                </div>
              </Card>
            ))}
          </div>
          <Card title={t("fees.ledger.receipts")} padded={false}>
            {data.data.receipts.length === 0 ? (
              <EmptyState title={t("fees.collections.empty")} />
            ) : (
              <table className="w-full text-sm">
                <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                  <tr>
                    <th className="px-3 py-2 text-start">{t("fees.receiptNo")}</th>
                    <th className="px-3 py-2 text-start">{t("students.fullName")}</th>
                    <th className="px-3 py-2 text-start">{t("fees.voucherNo")}</th>
                    <th className="px-3 py-2 text-start">{t("fees.pay.method")}</th>
                    <th className="px-3 py-2 text-start">{t("fees.print.receivedBy")}</th>
                    <th className="px-3 py-2 text-end">{t("fees.pay.amount")}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.data.receipts.map((r) => (
                    <tr key={r.id} className="border-t border-border">
                      <td className="px-3 py-1.5 font-mono text-xs">
                        <Link
                          to={`/fees/receipts/${r.id}/print`}
                          className="text-blue hover:underline"
                        >
                          {r.receipt_no}
                        </Link>{" "}
                        {r.reverses_id && <Badge tone="warning">{t("fees.reversed")}</Badge>}
                      </td>
                      <td className="px-3 py-1.5">
                        {r.student_name}{" "}
                        <span className="text-xs text-muted">{r.student_number}</span>
                      </td>
                      <td className="px-3 py-1.5 font-mono text-xs">{r.voucher_no}</td>
                      <td className="px-3 py-1.5">
                        {methodLabel(t, r.method)}
                        {r.reference ? ` · ${r.reference}` : ""}
                      </td>
                      <td className="px-3 py-1.5">{r.received_by_name}</td>
                      <td className="px-3 py-1.5 text-end">
                        <Amount
                          value={r.amount}
                          className={Number(r.amount) < 0 ? "text-danger" : ""}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Card>
        </div>
      ) : null}
    </div>
  );
}
