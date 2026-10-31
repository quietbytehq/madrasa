import { todayIso } from "@/lib/dates";
import { useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { PaymentMethod, ReceiptDetail, VoucherDetail } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useI18n } from "@/i18n";
import { formatMoney, parseMoney } from "@/lib/money";
import { errorMessage } from "@/lib/useAsync";
import { METHODS, methodLabel } from "./common";

/** Record a payment against a voucher. Validates locally; the server does the money maths. */
export function PayDialog({
  voucher,
  onClose,
  onPaid,
}: {
  voucher: VoucherDetail;
  onClose: () => void;
  onPaid: (r: ReceiptDetail, v: VoucherDetail) => void;
}) {
  const { t } = useI18n();
  const [amount, setAmount] = useState(voucher.balance);
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [paidOn, setPaidOn] = useState(todayIso());
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const overdue =
    paidOn > voucher.due_date && Number(voucher.late_fee) === 0 && !voucher.late_fee_waived;
  const lateFee = overdue
    ? (Number(voucher.net_monthly) * Number(voucher.policy.late_fee_percent)) / 100
    : 0;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const parsed = parseMoney(amount);
    if (!parsed || Number(parsed) <= 0)
      return setError(t("fees.pay.amount") + ": " + t("common.required"));
    if (method !== "cash" && !reference.trim())
      return setError(t("fees.pay.reference") + ": " + t("common.required"));
    setBusy(true);
    setError(null);
    try {
      const res = await api.payVoucher(voucher.id, {
        amount: parsed,
        method,
        reference: reference.trim() || undefined,
        note: note.trim() || undefined,
        paid_on: paidOn,
      });
      onPaid(res.receipt, res.voucher);
    } catch (err) {
      setError(errorMessage(err, t("app.error.generic")));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={t("fees.pay")}
      description={`${voucher.voucher_no} · ${voucher.student_name} · ${t("fees.balance")} ${formatMoney(voucher.balance)}`}
      size="sm"
    >
      <form onSubmit={submit} noValidate className="flex flex-col gap-3">
        {overdue && (
          <p
            role="status"
            className="rounded border border-warning bg-warning-bg px-2.5 py-2 text-xs text-text"
          >
            {t("fees.pay.lateWarning", { amount: formatMoney(lateFee.toFixed(2)) })}
          </p>
        )}
        <div className="flex items-end gap-2">
          <Input
            label={t("fees.pay.amount")}
            inputMode="decimal"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            className="flex-1"
            autoFocus
            required
          />
          <Button
            type="button"
            variant="secondary"
            size="md"
            onClick={() => setAmount(voucher.balance)}
          >
            {t("fees.pay.full")}
          </Button>
        </div>
        <Select
          label={t("fees.pay.method")}
          value={method}
          onChange={(e) => setMethod(e.target.value as PaymentMethod)}
          options={METHODS.map((m) => ({ value: m, label: methodLabel(t, m) }))}
        />
        <Input
          label={t("fees.pay.reference")}
          value={reference}
          onChange={(e) => setReference(e.target.value)}
          labelSuffix={method === "cash" ? t("field.optional") : undefined}
        />
        <Input
          label={t("fees.pay.date")}
          type="date"
          value={paidOn}
          onChange={(e) => setPaidOn(e.target.value)}
          max={todayIso()}
        />
        <Input
          label={t("fees.pay.note")}
          labelSuffix={t("field.optional")}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
        {error && (
          <p role="alert" className="rounded bg-danger-bg px-2.5 py-2 text-xs text-danger">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button type="submit" loading={busy}>
            {t("fees.pay")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
