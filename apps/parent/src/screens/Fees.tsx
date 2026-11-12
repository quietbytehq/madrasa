import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { formatMoney, isZero, monthLabel } from "@/lib/money";
import { useAsync } from "@/lib/useAsync";
import { hubApi } from "@parent/api";
import type { Voucher } from "@parent/api";
import { LastUpdated, Loadable, Panel } from "@parent/ui";
import { useChild } from "./Child";

const TONE: Record<Voucher["status"], BadgeTone> = {
  unpaid: "danger",
  partial: "warning",
  paid: "success",
  carried: "neutral",
};

/** Fee status and vouchers. Nothing is paid here: fees are paid at the school. */
export function FeesScreen() {
  const { t, locale, formatDate } = useI18n();
  const { child } = useChild();
  const data = useAsync(() => hubApi.fees(child), [child]);
  const intl = locale === "ur" ? "ur-PK" : "en-PK";
  const money = (value: string) => formatMoney(value, intl);
  const day = (value: string) => formatDate(`${value}T00:00:00`, { dateStyle: "medium" });
  return (
    <Loadable state={data}>
      {(f) => (
        <div className="flex flex-col gap-2">
          <Panel>
            <div className="text-xs text-muted">{t("parent.fees.due")}</div>
            <div className="text-2xl font-semibold text-text" data-testid="balance">
              <span dir="ltr">
                {f.currency} {money(f.balance)}
              </span>
            </div>
            <p className="mt-1 text-xs text-muted">
              {isZero(f.balance) ? t("parent.fees.nothingDue") : t("parent.fees.payAtSchool")}
            </p>
          </Panel>
          <h2 className="px-1 pt-1 text-sm font-semibold text-text">{t("parent.fees.vouchers")}</h2>
          {f.vouchers.length === 0 ? (
            <p className="px-4 py-6 text-center text-sm text-muted">{t("parent.fees.none")}</p>
          ) : (
            f.vouchers.map((v) => (
              <Panel
                key={v.id}
                title={monthLabel(v.billing_month, intl)}
                aside={
                  <Badge tone={TONE[v.status]}>
                    {t(`parent.fees.status.${v.status}` as TranslationKey)}
                  </Badge>
                }
              >
                <dl
                  className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 text-sm"
                  data-testid={`voucher-${v.voucher_no}`}
                >
                  <dt className="text-muted">{t("parent.fees.totalDue")}</dt>
                  <dd className="text-end text-text" dir="ltr">
                    {money(v.total_due)}
                  </dd>
                  {!isZero(v.late_fee) && (
                    <>
                      <dt className="text-muted">{t("parent.fees.lateFee")}</dt>
                      <dd className="text-end text-text" dir="ltr">
                        {money(v.late_fee)}
                      </dd>
                    </>
                  )}
                  <dt className="text-muted">{t("parent.fees.paid")}</dt>
                  <dd className="text-end text-text" dir="ltr">
                    {money(v.net_paid)}
                  </dd>
                  {v.status !== "carried" && (
                    <>
                      <dt className="font-semibold text-text">{t("parent.fees.balance")}</dt>
                      <dd className="text-end font-semibold text-text" dir="ltr">
                        {money(v.balance)}
                      </dd>
                    </>
                  )}
                </dl>
                <p className="mt-2 text-xs text-muted">
                  {v.status === "carried"
                    ? t("parent.fees.carried")
                    : t("parent.fees.dueBy", { date: day(v.due_date) })}
                </p>
                <details className="mt-2 text-sm">
                  <summary className="cursor-pointer py-1 text-xs font-medium text-blue">
                    {t("parent.fees.details")}
                  </summary>
                  <dl className="mt-1 grid grid-cols-[1fr_auto] gap-x-4 gap-y-1">
                    {v.lines.map((l, i) => (
                      <div key={i} className="contents">
                        <dt className="text-text">{l.description}</dt>
                        <dd className="text-end text-text" dir="ltr">
                          {money(l.amount)}
                        </dd>
                      </div>
                    ))}
                    {!isZero(v.sibling_discount) && (
                      <>
                        <dt className="text-muted">{t("parent.fees.siblingDiscount")}</dt>
                        <dd className="text-end text-text" dir="ltr">
                          −{money(v.sibling_discount)}
                        </dd>
                      </>
                    )}
                    {!isZero(v.concession_discount) && (
                      <>
                        <dt className="text-muted">{t("parent.fees.concession")}</dt>
                        <dd className="text-end text-text" dir="ltr">
                          −{money(v.concession_discount)}
                        </dd>
                      </>
                    )}
                    {!isZero(v.arrears) && (
                      <>
                        <dt className="text-muted">{t("parent.fees.arrears")}</dt>
                        <dd className="text-end text-text" dir="ltr">
                          {money(v.arrears)}
                        </dd>
                      </>
                    )}
                  </dl>
                  <p className="mt-1 text-xs text-muted">
                    {t("parent.fees.voucherNo")} <span dir="ltr">{v.voucher_no}</span>
                  </p>
                </details>
              </Panel>
            ))
          )}
          {f.receipts.length > 0 && (
            <Panel title={t("parent.fees.receipts")}>
              <ul className="divide-y divide-border">
                {f.receipts.map((r) => (
                  <li key={r.id} className="flex items-center justify-between gap-3 py-2 text-sm">
                    <span className="min-w-0">
                      <span className="block text-text">{day(r.paid_on)}</span>
                      <span className="block truncate text-xs text-muted">
                        <span dir="ltr">{r.receipt_no}</span>
                        {r.reversal && ` · ${t("parent.fees.reversal")}`}
                      </span>
                    </span>
                    <span className="font-medium text-text" dir="ltr">
                      {money(r.amount)}
                    </span>
                  </li>
                ))}
              </ul>
            </Panel>
          )}
          <LastUpdated at={f.synced_at} />
        </div>
      )}
    </Loadable>
  );
}
