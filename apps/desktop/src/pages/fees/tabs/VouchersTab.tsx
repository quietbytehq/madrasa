import { useState } from "react";
import type { FormEvent } from "react";
import { Link } from "react-router-dom";
import { api } from "@/api/endpoints";
import type { VoucherStatus } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { monthLabel, monthStart } from "@/lib/money";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { Amount, VoucherStatusBadge } from "../common";

const STATUSES: VoucherStatus[] = ["unpaid", "partial", "paid", "carried", "cancelled"];

export function VouchersTab() {
  const { t, formatDate } = useI18n();
  const { can } = useSession();
  const toast = useToast();
  const [month, setMonth] = useState(monthStart());
  const [classId, setClassId] = useState("");
  const [status, setStatus] = useState("");
  const [q, setQ] = useState("");
  const [generating, setGenerating] = useState(false);
  const classes = useAsync(() => api.classes(), []);
  const list = useAsync(
    () =>
      api.vouchers({
        month,
        class_id: classId || undefined,
        status: (status || undefined) as VoucherStatus | undefined,
        q: q || undefined,
      }),
    [month, classId, status, q],
  );

  const totals = (list.data ?? []).reduce(
    (acc, v) => ({
      due: acc.due + Number(v.total_due) + Number(v.late_fee),
      paid: acc.paid + Number(v.net_paid),
    }),
    { due: 0, paid: 0 },
  );

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <Input
          label={t("fees.month")}
          type="month"
          value={month.slice(0, 7)}
          onChange={(e) => setMonth(`${e.target.value}-01`)}
          className="w-44"
        />
        <Select
          label={t("common.class")}
          value={classId}
          onChange={(e) => setClassId(e.target.value)}
          options={[
            { value: "", label: t("common.all") },
            ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
          ]}
          className="w-36"
        />
        <Select
          label={t("common.status")}
          value={status}
          onChange={(e) => setStatus(e.target.value)}
          options={[
            { value: "", label: t("common.all") },
            ...STATUSES.map((s) => ({ value: s, label: t(`fees.status.${s}`) })),
          ]}
          className="w-40"
        />
        <Input
          label={t("common.search")}
          type="search"
          placeholder={t("fees.search")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          className="w-64"
        />
        <span className="flex-1" />
        {can("fees.manage") && (
          <Button onClick={() => setGenerating(true)}>{t("fees.generate")}</Button>
        )}
      </div>
      <Card padded={false}>
        {list.loading && !list.data ? (
          <div className="flex justify-center py-10">
            <Spinner />
          </div>
        ) : (list.data?.length ?? 0) === 0 ? (
          <EmptyState title={t("fees.empty")} />
        ) : (
          <table className="w-full text-sm">
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-3 py-2 text-start">{t("fees.voucherNo")}</th>
                <th className="px-3 py-2 text-start">{t("students.fullName")}</th>
                <th className="px-3 py-2 text-start">{t("students.classSection")}</th>
                <th className="px-3 py-2 text-start">{t("fees.dueDate")}</th>
                <th className="px-3 py-2 text-end">{t("fees.totalDue")}</th>
                <th className="px-3 py-2 text-end">{t("fees.lateFee")}</th>
                <th className="px-3 py-2 text-end">{t("fees.paid")}</th>
                <th className="px-3 py-2 text-start">{t("common.status")}</th>
              </tr>
            </thead>
            <tbody>
              {list.data!.map((v) => (
                <tr key={v.id} className="border-t border-border hover:bg-selection">
                  <td className="px-3 py-1.5 font-mono text-xs">
                    <Link to={`/fees/vouchers/${v.id}/print`} className="text-blue hover:underline">
                      {v.voucher_no}
                    </Link>
                  </td>
                  <td className="px-3 py-1.5">
                    <Link
                      to={`/students/${v.student_id}`}
                      className="font-medium text-text hover:underline"
                    >
                      {v.student_name}
                    </Link>{" "}
                    <span className="text-xs text-muted">{v.student_number}</span>
                  </td>
                  <td className="px-3 py-1.5">
                    {v.class_name ? `${v.class_name}-${v.section_name ?? ""}` : "—"}
                  </td>
                  <td className="px-3 py-1.5">{formatDate(v.due_date)}</td>
                  <td className="px-3 py-1.5 text-end">
                    <Amount value={v.total_due} />
                  </td>
                  <td className="px-3 py-1.5 text-end">
                    <Amount value={v.late_fee} />
                  </td>
                  <td className="px-3 py-1.5 text-end">
                    <Amount value={v.net_paid} />
                  </td>
                  <td className="px-3 py-1.5">
                    <VoucherStatusBadge status={v.status} t={t} />
                  </td>
                </tr>
              ))}
            </tbody>
            <tfoot className="bg-hover text-xs font-semibold text-text">
              <tr>
                <td className="px-3 py-2" colSpan={4}>
                  {t("common.results", { count: list.data!.length })} · {monthLabel(month)}
                </td>
                <td className="px-3 py-2 text-end">
                  <Amount value={totals.due.toFixed(2)} />
                </td>
                <td />
                <td className="px-3 py-2 text-end">
                  <Amount value={totals.paid.toFixed(2)} />
                </td>
                <td />
              </tr>
            </tfoot>
          </table>
        )}
      </Card>
      {generating && (
        <GenerateDialog
          month={month}
          classes={classes.data ?? []}
          onClose={() => setGenerating(false)}
          onDone={(r) => {
            setGenerating(false);
            toast.push({
              kind: "success",
              title: t("fees.generate.done", {
                created: r.created,
                skipped: r.skipped_existing,
                noStructure: r.skipped_no_structure,
              }),
            });
            void list.reload();
          }}
          onError={(err) =>
            toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) })
          }
        />
      )}
    </div>
  );
}

function GenerateDialog({
  month: initial,
  classes,
  onClose,
  onDone,
  onError,
}: {
  month: string;
  classes: Array<{ id: string; name: string }>;
  onClose: () => void;
  onDone: (r: { created: number; skipped_existing: number; skipped_no_structure: number }) => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [month, setMonth] = useState(initial);
  const [classId, setClassId] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      onDone(await api.generateVouchers({ month, class_id: classId || undefined }));
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open onClose={onClose} title={t("fees.generate")} size="sm">
      <form onSubmit={submit} noValidate className="flex flex-col gap-3">
        <Input
          label={t("fees.month")}
          type="month"
          value={month.slice(0, 7)}
          onChange={(e) => setMonth(`${e.target.value}-01`)}
          required
        />
        <Select
          label={t("fees.generate.scope")}
          value={classId}
          onChange={(e) => setClassId(e.target.value)}
          options={[
            { value: "", label: t("fees.generate.school") },
            ...classes.map((c) => ({ value: c.id, label: `${t("common.class")} ${c.name}` })),
          ]}
        />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button type="submit" loading={busy}>
            {t("fees.generate")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
