import { Printer } from "lucide-react";
import { useParams } from "react-router-dom";
import { api } from "@/api/endpoints";
import { provisionalReceipt } from "@/api/client";
import type { ProvisionalReceipt, ReceiptDetail, VoucherDetail } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useState } from "react";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { formatMoney, monthLabel } from "@/lib/money";
import { useMoney } from "@/lib/useMoney";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { methodLabel } from "./common";

/**
 * Print toolbar. With `onPdf` the Print button fetches the PDF typeset by the server (the
 * same engine as the reports, so amounts and names stay real text); without it the browser
 * print dialog is used (receipts).
 */
export function PrintToolbar({ title, onPdf }: { title: string; onPdf?: () => Promise<void> }) {
  const { t } = useI18n();
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const print = async () => {
    if (!onPdf) {
      window.print();
      return;
    }
    setBusy(true);
    try {
      await onPdf();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="mb-3 flex items-center gap-2 print:hidden">
      <h1 className="text-lg font-semibold text-navy">{title}</h1>
      <span className="flex-1" />
      <Button variant="secondary" onClick={() => window.history.back()}>
        {t("action.back")}
      </Button>
      <Button
        icon={<Printer className="h-4 w-4" aria-hidden />}
        loading={busy}
        onClick={() => void print()}
      >
        {onPdf ? t("fees.print.download") : `${t("action.print")} / ${t("fees.print.pdf")}`}
      </Button>
    </div>
  );
}

/** Voucher: school copy and parent copy on one sheet, with a bank/1Bill/Kuickpay reference box. */
export function VoucherPrintPage() {
  const { id = "" } = useParams();
  const { t, locale } = useI18n();
  const v = useAsync(() => api.voucher(id), [id]);
  if (v.loading && !v.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (v.error || !v.data)
    return <EmptyState title={errorMessage(v.error, t("common.error.load"))} />;
  return (
    <>
      <PrintToolbar
        title={`${t("fees.print.voucher")} ${v.data.voucher_no}`}
        onPdf={() => api.voucherPdf(id, locale === "ur" ? "ur" : "en")}
      />
      <p className="mb-2 text-xs text-muted print:hidden">{t("fees.print.downloadHint")}</p>
      <div className="print-area grid gap-4 xl:grid-cols-2 print:grid-cols-2 print:gap-2">
        <VoucherCopy v={v.data} copy={t("fees.print.schoolCopy")} />
        <VoucherCopy v={v.data} copy={t("fees.print.parentCopy")} />
      </div>
    </>
  );
}

function Row({ label, value, strong }: { label: string; value: string; strong?: boolean }) {
  return (
    <tr className={strong ? "font-semibold" : ""}>
      <td className="py-0.5 pe-2">{label}</td>
      <td className="py-0.5 text-end tabular-nums">{value}</td>
    </tr>
  );
}

function VoucherCopy({ v, copy }: { v: VoucherDetail; copy: string }) {
  const { t, formatDate } = useI18n();
  // Every amount carries the school's currency (v0.2.0-alpha B2).
  const formatMoney = useMoney();
  const discounts = Number(v.sibling_discount) + Number(v.concession_discount);
  return (
    <section className="rounded border border-border bg-card p-4 text-sm text-text print:rounded-none print:border-black">
      <header className="mb-2 flex items-start justify-between border-b border-border pb-2">
        <div>
          <div className="text-base font-bold text-navy">{v.school.name}</div>
          <div className="text-xs text-muted">
            {v.school.city} · {v.school.code}
          </div>
        </div>
        <div className="text-end text-xs">
          <div className="font-semibold uppercase">{copy}</div>
          <div className="font-mono">{v.voucher_no}</div>
        </div>
      </header>
      <dl className="mb-2 grid grid-cols-2 gap-x-3 gap-y-0.5 text-xs">
        <dt className="text-muted">{t("students.fullName")}</dt>
        <dd className="font-medium">{v.student_name}</dd>
        <dt className="text-muted">{t("students.fatherName")}</dt>
        <dd>{v.father_name}</dd>
        <dt className="text-muted">{t("students.number")}</dt>
        <dd className="font-mono">{v.student_number}</dd>
        <dt className="text-muted">{t("students.classSection")}</dt>
        <dd>{v.class_name ? `${v.class_name}-${v.section_name ?? ""}` : "—"}</dd>
        <dt className="text-muted">{t("fees.month")}</dt>
        <dd>{monthLabel(v.billing_month)}</dd>
        <dt className="text-muted">{t("fees.issueDate")}</dt>
        <dd>{formatDate(v.issue_date)}</dd>
        <dt className="text-muted">{t("fees.dueDate")}</dt>
        <dd className="font-semibold">{formatDate(v.due_date)}</dd>
      </dl>
      <table className="mb-2 w-full text-xs">
        <tbody>
          {v.lines.map((l) => (
            <Row key={l.id} label={l.description} value={formatMoney(l.amount)} />
          ))}
          <Row label={t("fees.gross")} value={formatMoney(v.gross)} strong />
          {Number(v.sibling_discount) > 0 && (
            <Row label={t("fees.siblingDiscount")} value={`− ${formatMoney(v.sibling_discount)}`} />
          )}
          {Number(v.concession_discount) > 0 && (
            <Row label={t("fees.concession")} value={`− ${formatMoney(v.concession_discount)}`} />
          )}
          {discounts > 0 && <Row label={t("fees.netMonthly")} value={formatMoney(v.net_monthly)} />}
          {Number(v.arrears) !== 0 && (
            <Row label={t("fees.arrears")} value={formatMoney(v.arrears)} />
          )}
          <Row label={t("fees.totalDue")} value={formatMoney(v.total_due)} strong />
          {Number(v.late_fee) > 0 && (
            <Row label={t("fees.lateFee")} value={formatMoney(v.late_fee)} />
          )}
          {Number(v.net_paid) > 0 && (
            <Row label={t("fees.paid")} value={`− ${formatMoney(v.net_paid)}`} />
          )}
          <Row label={t("fees.balance")} value={formatMoney(v.balance)} strong />
        </tbody>
      </table>
      <p className="mb-2 text-2xs text-muted">
        {t("fees.print.payBefore", {
          date: formatDate(v.due_date),
          percent: v.policy.late_fee_percent,
        })}
      </p>
      <div className="rounded border border-dashed border-border p-2 text-2xs text-muted print:border-black">
        <div className="font-semibold uppercase">{v.payment_reference.label}</div>
        <div className="mt-1 font-mono text-sm text-text">{v.payment_reference.reference}</div>
        {v.payment_reference.note && <div className="mt-1">{v.payment_reference.note}</div>}
        <div className="mt-3 flex justify-between">
          <span>{t("fees.print.receivedBy")}: ____________</span>
          <span>{t("fees.print.signature")}: ____________</span>
        </div>
      </div>
    </section>
  );
}

/** Receipt reprint. */
export function ReceiptPrintPage() {
  const { id = "" } = useParams();
  const { t, formatDate } = useI18n();
  const r = useAsync(() => api.receipt(id), [id]);
  if (r.loading && !r.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (r.error || !r.data)
    return <EmptyState title={errorMessage(r.error, t("common.error.load"))} />;
  const d: ReceiptDetail = r.data;
  const reversal = Number(d.amount) < 0;
  return (
    <>
      <PrintToolbar title={`${t("fees.print.receipt")} ${d.receipt_no}`} />
      <section className="print-area mx-auto max-w-lg rounded border border-border bg-card p-4 text-sm text-text print:rounded-none print:border-black">
        <header className="mb-2 flex items-start justify-between border-b border-border pb-2">
          <div>
            <div className="text-base font-bold text-navy">{d.school.name}</div>
            <div className="text-xs text-muted">
              {d.school.city} · {d.school.code}
            </div>
          </div>
          <div className="text-end text-xs">
            <div className="font-semibold uppercase">
              {reversal ? t("fees.reversed") : t("fees.print.receipt")}
            </div>
            <div className="font-mono">{d.receipt_no}</div>
            <div>{formatDate(d.paid_on)}</div>
          </div>
        </header>
        <dl className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-xs">
          <dt className="text-muted">{t("students.fullName")}</dt>
          <dd className="font-medium">{d.student_name}</dd>
          <dt className="text-muted">{t("students.fatherName")}</dt>
          <dd>{d.father_name}</dd>
          <dt className="text-muted">{t("students.number")}</dt>
          <dd className="font-mono">{d.student_number}</dd>
          <dt className="text-muted">{t("students.classSection")}</dt>
          <dd>{d.class_name ? `${d.class_name}-${d.section_name ?? ""}` : "—"}</dd>
          <dt className="text-muted">{t("fees.voucherNo")}</dt>
          <dd className="font-mono">
            {d.voucher_no} · {monthLabel(d.billing_month)}
          </dd>
          <dt className="text-muted">{t("fees.pay.method")}</dt>
          <dd>
            {methodLabel(t, d.method)}
            {d.reference ? ` · ${d.reference}` : ""}
          </dd>
          {d.reversal_reason && (
            <>
              <dt className="text-muted">{t("fees.reverse.reason")}</dt>
              <dd>{d.reversal_reason}</dd>
            </>
          )}
          {d.note && (
            <>
              <dt className="text-muted">{t("fees.pay.note")}</dt>
              <dd>{d.note}</dd>
            </>
          )}
        </dl>
        <div className="mt-3 flex items-baseline justify-between border-t border-border pt-2">
          <span className="text-xs uppercase text-muted">{t("fees.print.amountInWords")}</span>
          <span
            className={`text-xl font-bold tabular-nums ${reversal ? "text-danger" : "text-navy"}`}
          >
            {d.school.currency} {formatMoney(d.amount)}
          </span>
        </div>
        <div className="mt-1 flex justify-between text-xs text-muted">
          <span>
            {t("fees.balance")}: {formatMoney(d.voucher_balance_after)}
          </span>
          <span>
            {t("fees.print.receivedBy")}: {d.received_by_name}
          </span>
        </div>
        {d.reversed_by && (
          <p className="mt-2 rounded bg-warning-bg px-2 py-1 text-xs text-text">
            {t("fees.reversed")}: {d.reversed_by.receipt_no}
          </p>
        )}
        {d.provisional_no && (
          <p className="mt-2 text-xs text-muted" data-testid="replaces-provisional">
            {t("fees.provisional.replaces", { number: d.provisional_no })}
          </p>
        )}
        <div className="mt-6 flex justify-between text-xs text-muted print:mt-10">
          <span>{t("fees.print.signature")}: ____________</span>
          <span>{t("fees.print.signature")}: ____________</span>
        </div>
      </section>
    </>
  );
}

/**
 * A receipt taken while the school server was out of reach (LAN client, ADR 0009): printed at
 * once with its provisional number, marked clearly; once the server has it, the page shows the
 * real number that replaced it.
 */
export function ProvisionalReceiptPage() {
  const { number = "" } = useParams();
  const { t, formatDate } = useI18n();
  const r = useAsync(() => provisionalReceipt(number), [number]);
  if (r.loading && !r.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (r.error || !r.data) return <EmptyState title={t("fees.provisional.notFound")} />;
  const p: ProvisionalReceipt = r.data;
  const v = p.voucher;
  return (
    <>
      <PrintToolbar title={`${t("fees.provisional.title")} ${p.provisional_no}`} />
      <section
        className="print-area mx-auto max-w-lg rounded border-2 border-dashed border-warning bg-card p-4 text-sm text-text print:rounded-none print:border-black"
        data-testid="provisional-receipt"
      >
        <p className="mb-2 rounded bg-warning-bg px-2 py-1.5 text-center text-sm font-bold uppercase tracking-wide text-text print:border print:border-black">
          {t("fees.provisional.banner")}
        </p>
        <header className="mb-2 flex items-start justify-between border-b border-border pb-2">
          <div className="flex items-start gap-2">
            <div>
              <div className="text-base font-bold text-navy">{v?.school.name ?? ""}</div>
              <div className="text-xs text-muted">
                {v ? `${v.school.city} · ${v.school.code}` : ""}
              </div>
            </div>
          </div>
          <div className="text-end text-xs">
            <div className="font-semibold uppercase">{t("fees.provisional.title")}</div>
            <div className="font-mono" data-testid="provisional-number">
              {p.provisional_no}
            </div>
            <div>{formatDate(p.paid_on)}</div>
          </div>
        </header>
        <dl className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-xs">
          {v && (
            <>
              <dt className="text-muted">{t("students.fullName")}</dt>
              <dd className="font-medium">{v.student_name}</dd>
              <dt className="text-muted">{t("students.fatherName")}</dt>
              <dd>{v.father_name}</dd>
              <dt className="text-muted">{t("students.number")}</dt>
              <dd className="font-mono">{v.student_number}</dd>
              <dt className="text-muted">{t("fees.voucherNo")}</dt>
              <dd className="font-mono">
                {v.voucher_no} · {monthLabel(v.billing_month)}
              </dd>
            </>
          )}
          <dt className="text-muted">{t("fees.pay.method")}</dt>
          <dd>
            {methodLabel(t, p.method)}
            {p.reference ? ` · ${p.reference}` : ""}
          </dd>
          {p.note && (
            <>
              <dt className="text-muted">{t("fees.pay.note")}</dt>
              <dd>{p.note}</dd>
            </>
          )}
        </dl>
        <div className="mt-3 flex items-baseline justify-between border-t border-border pt-2">
          <span className="text-xs uppercase text-muted">{t("fees.print.amountInWords")}</span>
          <span className="text-xl font-bold tabular-nums text-navy">
            {v?.school.currency ?? ""} {formatMoney(p.amount)}
          </span>
        </div>
        <p className="mt-3 text-xs" data-testid="provisional-status">
          {p.receipt_no
            ? t("fees.provisional.replacedBy", { number: p.receipt_no })
            : t("fees.provisional.pending")}
        </p>
        <div className="mt-6 flex justify-between text-xs text-muted print:mt-10">
          <span>{t("fees.print.signature")}: ____________</span>
        </div>
      </section>
    </>
  );
}
