import { useState } from "react";
import type { FormEvent } from "react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { hub } from "../api";
import type { EscrowOverview, ReleasedKey } from "../api";
import { useFail, useHubSession } from "../session";
import { td, th, useWhen } from "../format";
import { PageTitle } from "./common";

type Deposit = EscrowOverview["deposits"][number];

/**
 * Backup key escrow: set it up, see which schools have left a key, release one with the
 * passphrase and a reason, and see every release. The passphrase and a released key are
 * held only while their dialog is open.
 */
export function EscrowPage() {
  const { t } = useI18n();
  const { can } = useHubSession();
  const when = useWhen();
  const toast = useToast();
  const overview = useAsync(() => hub.escrow(), []);
  const [releasing, setReleasing] = useState<Deposit | null>(null);
  const [released, setReleased] = useState<ReleasedKey | null>(null);
  const [changing, setChanging] = useState(false);
  const d = overview.data;

  return (
    <>
      <PageTitle
        title={t("hub.nav.escrow")}
        about={t("hub.escrow.about")}
        actions={
          d?.set_up &&
          can("nexus.escrow") && (
            <Button variant="secondary" onClick={() => setChanging(true)}>
              {t("hub.escrow.changePassphrase")}
            </Button>
          )
        }
      />
      {overview.loading && !d && <Spinner />}
      {d && !d.set_up && (
        <SetupCard
          allowed={can("nexus.admin")}
          onDone={() => {
            toast.push({ kind: "success", title: t("hub.escrow.setUp") });
            void overview.reload();
          }}
        />
      )}
      {d?.set_up && (
        <>
          <Card title={t("hub.escrow.deposits")} padded={false}>
            {d.deposits.length === 0 ? (
              <p className="p-4 text-sm text-muted">{t("hub.escrow.noDeposits")}</p>
            ) : (
              <table className="w-full">
                <thead className="border-b border-border">
                  <tr>
                    <th className={th}>{t("hub.schools.school")}</th>
                    <th className={th}>{t("hub.escrow.since")}</th>
                    <th className={th} />
                  </tr>
                </thead>
                <tbody>
                  {d.deposits.map((x) => (
                    <tr key={x.school_id} className="border-t border-border">
                      <td className={td}>
                        <div className="font-medium">{x.school_name}</div>
                        <div className="text-xs text-muted" dir="ltr">
                          {x.school_code}
                        </div>
                      </td>
                      <td className={td}>{when(x.deposited_at)}</td>
                      <td className={`${td} text-end`}>
                        {can("nexus.escrow") && (
                          <Button variant="danger" size="sm" onClick={() => setReleasing(x)}>
                            {t("hub.escrow.release")}
                          </Button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Card>
          <Card title={t("hub.escrow.releases")} padded={false} className="mt-4">
            {d.releases.length === 0 ? (
              <p className="p-4 text-sm text-muted">{t("hub.escrow.noReleases")}</p>
            ) : (
              <table className="w-full">
                <thead className="border-b border-border">
                  <tr>
                    <th className={th}>{t("nexus.log.when")}</th>
                    <th className={th}>{t("hub.schools.school")}</th>
                    <th className={th}>{t("hub.escrow.by")}</th>
                    <th className={th}>{t("hub.school.reason")}</th>
                    <th className={th}>{t("hub.escrow.schoolTold")}</th>
                  </tr>
                </thead>
                <tbody>
                  {d.releases.map((r) => (
                    <tr key={r.id} className="border-t border-border">
                      <td className={`${td} whitespace-nowrap`}>{when(r.released_at)}</td>
                      <td className={td}>{r.school_name}</td>
                      <td className={td}>{r.released_by}</td>
                      <td className={td}>{r.reason}</td>
                      <td className={td}>
                        {r.seen_at ? (
                          when(r.seen_at)
                        ) : (
                          <Badge tone="warning">{t("hub.escrow.notYet")}</Badge>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Card>
        </>
      )}

      {releasing && (
        <ReleaseDialog
          deposit={releasing}
          onClose={() => setReleasing(null)}
          onReleased={(key) => {
            setReleasing(null);
            setReleased(key);
            void overview.reload();
          }}
        />
      )}
      <Dialog
        open={released !== null}
        onClose={() => setReleased(null)}
        title={t("hub.escrow.released.title", { school: released?.school_name ?? "" })}
        description={t("hub.escrow.released.once")}
        footer={<Button onClick={() => setReleased(null)}>{t("hub.escrow.released.done")}</Button>}
      >
        <p
          dir="ltr"
          data-testid="released-key"
          className="select-all break-all rounded border border-border bg-hover p-3 text-center font-mono text-sm"
        >
          {released?.released_key}
        </p>
        <p className="mt-2 text-xs text-muted">{t("hub.escrow.released.how")}</p>
      </Dialog>
      {changing && (
        <PassphraseDialog
          onClose={() => setChanging(false)}
          onDone={() => {
            setChanging(false);
            toast.push({ kind: "success", title: t("hub.escrow.passphraseChanged") });
          }}
        />
      )}
    </>
  );
}

function SetupCard({ allowed, onDone }: { allowed: boolean; onDone: () => void }) {
  const { t } = useI18n();
  const fail = useFail();
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const mismatch = confirm !== "" && passphrase !== confirm;
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await hub.escrowSetup(passphrase);
      onDone();
    } catch (err) {
      fail(err);
      setBusy(false);
    }
  };
  return (
    <Card title={t("hub.escrow.setup.title")}>
      <p
        role="note"
        className="mb-3 rounded border border-warning/60 bg-warning-bg px-3 py-2 text-xs text-text"
      >
        {t("hub.escrow.setup.warning")}
      </p>
      {!allowed ? (
        <p className="text-sm text-muted">{t("hub.escrow.setup.adminOnly")}</p>
      ) : (
        <form onSubmit={(e) => void submit(e)} noValidate className="flex max-w-md flex-col gap-3">
          <Input
            label={t("hub.escrow.passphrase")}
            hint={t("hub.escrow.passphraseHint")}
            type="password"
            value={passphrase}
            onChange={(e) => setPassphrase(e.target.value)}
            autoComplete="off"
          />
          <Input
            label={t("hub.escrow.passphraseAgain")}
            type="password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            autoComplete="off"
            error={mismatch ? t("hub.escrow.passphraseMismatch") : undefined}
          />
          <div>
            <Button
              type="submit"
              loading={busy}
              disabled={passphrase.length < 12 || passphrase !== confirm}
            >
              {t("hub.escrow.setup.submit")}
            </Button>
          </div>
        </form>
      )}
    </Card>
  );
}

function ReleaseDialog({
  deposit,
  onClose,
  onReleased,
}: {
  deposit: Deposit;
  onClose: () => void;
  onReleased: (key: ReleasedKey) => void;
}) {
  const { t } = useI18n();
  const fail = useFail();
  const [passphrase, setPassphrase] = useState("");
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      onReleased(await hub.releaseKey(deposit.school_id, passphrase, reason.trim()));
    } catch (err) {
      fail(err);
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("hub.escrow.release.title", { school: deposit.school_name })}
      description={t("hub.escrow.release.about")}
    >
      <form onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-3">
        <Input
          label={t("hub.school.reason")}
          hint={t("hub.escrow.release.reasonHint")}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          autoFocus
        />
        <Input
          label={t("hub.escrow.passphrase")}
          type="password"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          autoComplete="off"
        />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            type="submit"
            variant="danger"
            loading={busy}
            disabled={!reason.trim() || !passphrase}
          >
            {t("hub.escrow.release")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

function PassphraseDialog({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t } = useI18n();
  const fail = useFail();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const mismatch = confirm !== "" && next !== confirm;
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await hub.escrowPassphrase(current, next);
      onDone();
    } catch (err) {
      fail(err);
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("hub.escrow.changePassphrase")}
      description={t("hub.escrow.changeAbout")}
    >
      <form onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-3">
        <Input
          label={t("hub.escrow.currentPassphrase")}
          type="password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
          autoComplete="off"
          autoFocus
        />
        <Input
          label={t("hub.escrow.newPassphrase")}
          hint={t("hub.escrow.passphraseHint")}
          type="password"
          value={next}
          onChange={(e) => setNext(e.target.value)}
          autoComplete="off"
        />
        <Input
          label={t("hub.escrow.passphraseAgain")}
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          autoComplete="off"
          error={mismatch ? t("hub.escrow.passphraseMismatch") : undefined}
        />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            type="submit"
            loading={busy}
            disabled={!current || next.length < 12 || next !== confirm}
          >
            {t("action.save")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
