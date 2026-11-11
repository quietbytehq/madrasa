import { useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { NexusEscrowView } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

/**
 * Settings → Nexus → the backup key and head office. A school may leave its backup key
 * with head office (sealed, so head office needs its own passphrase to open it), take it
 * back, and always sees when head office released it, by whom and why.
 */
export function NexusEscrow({ canManageBackups }: { canManageBackups: boolean }) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const escrow = useAsync(() => api.nexusEscrow(), []);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [withdrawing, setWithdrawing] = useState(false);

  const when = (value: string | null) =>
    value ? formatDate(value, { dateStyle: "medium", timeStyle: "short" }) : "";
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });

  const deposit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      escrow.setData(await api.nexusEscrowDeposit(password));
      setPassword("");
      toast.push({ kind: "success", title: t("nexus.escrow.deposited") });
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };

  const withdraw = async () => {
    setBusy(true);
    try {
      escrow.setData(await api.nexusEscrowWithdraw());
      setWithdrawing(false);
      toast.push({ kind: "success", title: t("nexus.escrow.withdrawn") });
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };

  const d: NexusEscrowView | null = escrow.data;
  return (
    <section aria-labelledby="nexus-escrow-title" className="border-t border-border pt-3">
      <h3 id="nexus-escrow-title" className="text-xs font-semibold text-navy">
        {t("nexus.escrow.title")}
      </h3>
      <p className="mt-1 text-xs text-muted">{t("nexus.escrow.hint")}</p>

      {escrow.loading && !d && <Spinner />}
      {!escrow.loading && !d && (
        <p role="status" className="mt-2 text-xs text-muted">
          {t("nexus.escrow.unreachable")}{" "}
          <button
            type="button"
            className="text-blue hover:underline"
            onClick={() => void escrow.reload()}
          >
            {t("nexus.escrow.retry")}
          </button>
        </p>
      )}

      {d && d.releases.length > 0 && (
        <div
          role="alert"
          className="mt-2 rounded border border-warning/60 bg-warning-bg p-2 text-xs text-text"
        >
          <div className="font-semibold">{t("nexus.escrow.released.title")}</div>
          <ul className="mt-1 list-disc ps-5">
            {d.releases.map((r) => (
              <li key={r.id}>
                {t("nexus.escrow.released.item", {
                  date: when(r.released_at),
                  by: r.released_by,
                  reason: r.reason,
                })}
              </li>
            ))}
          </ul>
          <p className="mt-1">{t("nexus.escrow.released.hint")}</p>
        </div>
      )}

      {d && !d.available && !d.deposited && (
        <p className="mt-2 text-xs text-text" data-testid="nexus-escrow-state">
          {t("nexus.escrow.notAvailable")}
        </p>
      )}

      {d && d.deposited && (
        <div className="mt-2 flex flex-col gap-2 text-xs">
          <p data-testid="nexus-escrow-state" className="text-text">
            {t("nexus.escrow.held", { date: when(d.deposited_at) })}
          </p>
          {!d.current && (
            <p role="alert" className="rounded border border-warning/60 bg-warning-bg p-2">
              {t("nexus.escrow.stale")}
            </p>
          )}
          {canManageBackups && (
            <div>
              <Button variant="secondary" size="sm" onClick={() => setWithdrawing(true)}>
                {t("nexus.escrow.withdraw")}
              </Button>
            </div>
          )}
        </div>
      )}

      {d && d.available && (!d.deposited || !d.current) && canManageBackups && (
        <form onSubmit={(e) => void deposit(e)} className="mt-2 flex flex-wrap items-end gap-2">
          <div className="min-w-56 flex-1">
            <Input
              label={t("backup.restore.password")}
              hint={t("nexus.escrow.passwordHint")}
              type="password"
              autoComplete="off"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </div>
          <Button type="submit" loading={busy} disabled={!password}>
            {t("nexus.escrow.deposit")}
          </Button>
        </form>
      )}
      {d && d.available && !d.deposited && !canManageBackups && (
        <p className="mt-2 text-xs text-muted" data-testid="nexus-escrow-state">
          {t("nexus.escrow.needsBackupRole")}
        </p>
      )}

      <Dialog
        open={withdrawing}
        onClose={() => setWithdrawing(false)}
        title={t("nexus.escrow.withdraw.title")}
        description={t("nexus.escrow.withdraw.body")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setWithdrawing(false)}>
              {t("action.cancel")}
            </Button>
            <Button variant="danger" loading={busy} onClick={() => void withdraw()}>
              {t("nexus.escrow.withdraw")}
            </Button>
          </>
        }
      />
    </section>
  );
}
