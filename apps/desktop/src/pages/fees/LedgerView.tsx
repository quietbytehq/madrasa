import { useState } from "react";
import type { FormEvent } from "react";
import { Printer } from "lucide-react";
import { Link } from "react-router-dom";
import { api } from "@/api/endpoints";
import type { ConcessionKind, Ledger, VoucherDetail } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { monthLabel } from "@/lib/money";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { Amount, methodLabel, VoucherStatusBadge } from "./common";
import { PayDialog } from "./PayDialog";

/** A student's fee ledger with actions: pay, waive, reverse, concessions, print. */
export function LedgerView({ studentId }: { studentId: string }) {
  const { t, formatDate } = useI18n();
  const { can } = useSession();
  const toast = useToast();
  const ledger = useAsync(() => api.ledger(studentId), [studentId]);
  const [paying, setPaying] = useState<VoucherDetail | null>(null);
  const [reason, setReason] = useState<{ kind: "waive" | "cancel" | "reverse"; id: string } | null>(
    null,
  );
  const [reasonText, setReasonText] = useState("");
  const [addingConcession, setAddingConcession] = useState(false);
  const [busy, setBusy] = useState(false);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const manage = can("fees.manage");
  const approve = can("fees.approve");

  const openPay = async (voucherId: string) => {
    try {
      setPaying(await api.voucher(voucherId));
    } catch (err) {
      fail(err);
    }
  };

  const submitReason = async (e: FormEvent) => {
    e.preventDefault();
    if (!reason || !reasonText.trim()) return;
    setBusy(true);
    try {
      if (reason.kind === "waive") await api.waiveLateFee(reason.id, reasonText.trim());
      else if (reason.kind === "cancel") await api.cancelVoucher(reason.id, reasonText.trim());
      else await api.reverseReceipt(reason.id, reasonText.trim());
      setReason(null);
      setReasonText("");
      await ledger.reload();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };

  if (ledger.loading && !ledger.data) {
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  }
  if (ledger.error || !ledger.data)
    return <EmptyState title={errorMessage(ledger.error, t("common.error.load"))} />;
  const l: Ledger = ledger.data;
  const reasonTitle =
    reason?.kind === "waive"
      ? t("fees.waive")
      : reason?.kind === "cancel"
        ? t("fees.cancel")
        : t("fees.reverse");

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
        {(["billed", "discounts", "fines", "paid", "balance"] as const).map((k) => (
          <Card key={k} padded={false} className="p-3">
            <div className="text-2xs uppercase tracking-wide text-muted">{t(`fees.${k}`)}</div>
            <div
              className={`mt-1 text-lg font-semibold tabular-nums ${k === "balance" && Number(l.totals.balance) > 0 ? "text-danger" : "text-navy"}`}
            >
              <Amount value={l.totals[k]} />
            </div>
          </Card>
        ))}
      </div>

      <Card title={t("fees.tabs.vouchers")} padded={false}>
        {l.vouchers.length === 0 ? (
          <EmptyState title={t("fees.ledger.empty")} />
        ) : (
          <table className="w-full text-sm">
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2 text-start">{t("fees.month")}</th>
                <th className="px-3 py-2 text-start">{t("fees.voucherNo")}</th>
                <th className="px-3 py-2 text-end">{t("fees.gross")}</th>
                <th className="px-3 py-2 text-end">{t("fees.discounts")}</th>
                <th className="px-3 py-2 text-end">{t("fees.arrears")}</th>
                <th className="px-3 py-2 text-end">{t("fees.lateFee")}</th>
                <th className="px-3 py-2 text-end">{t("fees.paid")}</th>
                <th className="px-3 py-2 text-end">{t("fees.balance")}</th>
                <th className="px-3 py-2 text-start">{t("common.status")}</th>
                <th className="px-3 py-2 text-end">{t("common.actions")}</th>
              </tr>
            </thead>
            <tbody>
              {l.vouchers.map((v) => (
                <tr key={v.id} className="border-t border-border">
                  <td className="px-3 py-1.5">{monthLabel(v.billing_month)}</td>
                  <td className="px-3 py-1.5 font-mono text-xs">
                    <Link to={`/fees/vouchers/${v.id}/print`} className="text-blue hover:underline">
                      {v.voucher_no}
                    </Link>
                  </td>
                  <td className="px-3 py-1.5 text-end">
                    <Amount value={v.gross} />
                  </td>
                  <td className="px-3 py-1.5 text-end">
                    <Amount
                      value={String(Number(v.sibling_discount) + Number(v.concession_discount))}
                    />
                  </td>
                  <td className="px-3 py-1.5 text-end">
                    <Amount value={v.arrears} />
                  </td>
                  <td className="px-3 py-1.5 text-end">
                    <Amount value={v.late_fee} />
                    {v.late_fee_waived && (
                      <Badge tone="info" className="ms-1">
                        {t("fees.lateFee.waived")}
                      </Badge>
                    )}
                  </td>
                  <td className="px-3 py-1.5 text-end">
                    <Amount value={v.net_paid} />
                  </td>
                  <td className="px-3 py-1.5 text-end font-medium">
                    <Amount value={v.balance} />
                  </td>
                  <td className="px-3 py-1.5">
                    <VoucherStatusBadge status={v.status} t={t} />
                  </td>
                  <td className="px-3 py-1.5 text-end">
                    <div className="flex justify-end gap-1">
                      {manage && l.payable.includes(v.id) && (
                        <Button size="sm" onClick={() => void openPay(v.id)}>
                          {t("fees.pay")}
                        </Button>
                      )}
                      {approve &&
                        Number(v.late_fee) > 0 &&
                        !v.late_fee_waived &&
                        (v.status === "unpaid" || v.status === "partial") && (
                          <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => setReason({ kind: "waive", id: v.id })}
                          >
                            {t("fees.waive")}
                          </Button>
                        )}
                      {approve && v.status === "unpaid" && Number(v.net_paid) === 0 && (
                        <Button
                          size="sm"
                          variant="ghost"
                          className="text-danger"
                          onClick={() => setReason({ kind: "cancel", id: v.id })}
                        >
                          {t("fees.cancel")}
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <div className="grid gap-4 xl:grid-cols-2">
        <Card title={t("fees.ledger.receipts")} padded={false}>
          {l.receipts.length === 0 ? (
            <EmptyState title={t("fees.ledger.receipts")} />
          ) : (
            <ul className="divide-y divide-border text-sm">
              {l.receipts.map((r) => (
                <li key={r.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2">
                  <Link
                    to={`/fees/receipts/${r.id}/print`}
                    className="font-mono text-xs text-blue hover:underline"
                  >
                    {r.receipt_no}
                  </Link>
                  <span className="text-muted">{formatDate(r.paid_on)}</span>
                  <span>
                    {methodLabel(t, r.method)}
                    {r.reference ? ` · ${r.reference}` : ""}
                  </span>
                  {r.reverses_id && <Badge tone="warning">{t("fees.reversed")}</Badge>}
                  <span className="flex-1" />
                  <Amount value={r.amount} className={Number(r.amount) < 0 ? "text-danger" : ""} />
                  <Link
                    to={`/fees/receipts/${r.id}/print`}
                    aria-label={t("fees.reprint")}
                    className="rounded p-1 text-muted hover:bg-hover hover:text-text"
                  >
                    <Printer className="h-4 w-4" aria-hidden />
                  </Link>
                  {approve && !r.reverses_id && !l.receipts.some((x) => x.reverses_id === r.id) && (
                    <Button
                      size="sm"
                      variant="ghost"
                      className="text-danger"
                      onClick={() => setReason({ kind: "reverse", id: r.id })}
                    >
                      {t("fees.reverse")}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </Card>
        <Card
          title={t("fees.ledger.concessions")}
          padded={false}
          actions={
            approve && (
              <Button size="sm" onClick={() => setAddingConcession(true)}>
                {t("fees.concession.add")}
              </Button>
            )
          }
        >
          {l.concessions.length === 0 ? (
            <EmptyState title={t("fees.ledger.concessions")} />
          ) : (
            <ul className="divide-y divide-border text-sm">
              {l.concessions.map((c) => (
                <li key={c.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2">
                  <Badge tone="lavender">{t(`fees.concession.kind.${c.kind}`)}</Badge>
                  <span>
                    {c.percent ? `${c.percent}%` : ""}
                    {c.amount ? <Amount value={c.amount} /> : ""}
                  </span>
                  <span className="text-muted">{c.reason}</span>
                  <span className="text-xs text-muted">
                    {t("fees.concession.validFrom")} {formatDate(c.valid_from)}
                  </span>
                  <span className="flex-1" />
                  {approve && (
                    <Button
                      size="sm"
                      variant="ghost"
                      className="text-danger"
                      onClick={() =>
                        void api
                          .endConcession(c.id)
                          .then(() => ledger.reload())
                          .catch(fail)
                      }
                    >
                      {t("fees.concession.end")}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>

      {paying && (
        <PayDialog
          voucher={paying}
          onClose={() => setPaying(null)}
          onPaid={(r) => {
            setPaying(null);
            toast.push({ kind: "success", title: t("fees.pay.success", { number: r.receipt_no }) });
            void ledger.reload();
          }}
        />
      )}

      <Dialog
        open={!!reason}
        onClose={() => setReason(null)}
        title={reasonTitle}
        description={reason?.kind === "reverse" ? t("fees.reverse.body") : undefined}
        size="sm"
      >
        <form onSubmit={submitReason} noValidate className="flex flex-col gap-3">
          <Input
            label={
              reason?.kind === "waive"
                ? t("fees.waive.reason")
                : reason?.kind === "cancel"
                  ? t("fees.cancel.reason")
                  : t("fees.reverse.reason")
            }
            value={reasonText}
            onChange={(e) => setReasonText(e.target.value)}
            autoFocus
            required
          />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setReason(null)}>
              {t("action.cancel")}
            </Button>
            <Button type="submit" variant="danger" loading={busy} disabled={!reasonText.trim()}>
              {reasonTitle}
            </Button>
          </div>
        </form>
      </Dialog>

      {addingConcession && (
        <ConcessionDialog
          studentId={studentId}
          onClose={() => setAddingConcession(false)}
          onSaved={() => {
            setAddingConcession(false);
            void ledger.reload();
          }}
          onError={fail}
        />
      )}
    </div>
  );
}

function ConcessionDialog({
  studentId,
  onClose,
  onSaved,
  onError,
}: {
  studentId: string;
  onClose: () => void;
  onSaved: () => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [kind, setKind] = useState<ConcessionKind>("scholarship");
  const [percent, setPercent] = useState("");
  const [amount, setAmount] = useState("");
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!reason.trim() || (!percent && !amount)) return;
    setBusy(true);
    try {
      await api.createConcession(studentId, {
        kind,
        percent: percent || undefined,
        amount: amount || undefined,
        reason: reason.trim(),
      });
      onSaved();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open onClose={onClose} title={t("fees.concession.add")} size="sm">
      <form onSubmit={submit} noValidate className="flex flex-col gap-3">
        <Select
          label={t("fees.concession.kind")}
          value={kind}
          onChange={(e) => setKind(e.target.value as ConcessionKind)}
          options={(["staff_child", "scholarship", "custom"] as const).map((k) => ({
            value: k,
            label: t(`fees.concession.kind.${k}`),
          }))}
        />
        <Input
          label={t("fees.concession.percent")}
          inputMode="decimal"
          value={percent}
          onChange={(e) => setPercent(e.target.value)}
        />
        <Input
          label={t("fees.concession.amount")}
          inputMode="decimal"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
        <Input
          label={t("fees.concession.reason")}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          required
        />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button type="submit" loading={busy} disabled={!reason.trim() || (!percent && !amount)}>
            {t("action.save")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
