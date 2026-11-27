import { useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/useAsync";

const MIN_LENGTH = 10;

/**
 * Change one's own password (user menu). The current password is asked for; the server ends
 * every other session of the user. Until v0.5.1 only an administrator could set a password,
 * so the administrator knew everyone's.
 *
 * The head office console uses the same dialog with its own `change` (the hub's API).
 */
export function ChangePasswordDialog({
  onClose,
  change = api.changeOwnPassword,
}: {
  onClose: () => void;
  change?: (current: string, next: string) => Promise<unknown>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (next.length < MIN_LENGTH) return setError(t("password.change.tooShort"));
    if (next !== again) return setError(t("password.change.mismatch"));
    setBusy(true);
    setError(null);
    try {
      await change(current, next);
      toast.push({ kind: "success", title: t("password.change.done") });
      onClose();
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
      title={t("password.change.title")}
      description={t("password.change.help")}
      size="sm"
    >
      {/* Passwords are never kept as a draft (data-no-draft). */}
      <form onSubmit={submit} noValidate data-no-draft className="flex flex-col gap-3">
        <Input
          label={t("password.change.current")}
          type="password"
          autoComplete="current-password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
          autoFocus
          required
        />
        <Input
          label={t("password.change.new")}
          type="password"
          autoComplete="new-password"
          value={next}
          onChange={(e) => setNext(e.target.value)}
          hint={t("password.change.rule")}
          required
        />
        <Input
          label={t("password.change.again")}
          type="password"
          autoComplete="new-password"
          value={again}
          onChange={(e) => setAgain(e.target.value)}
          required
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
          <Button type="submit" loading={busy} disabled={!current || !next || !again}>
            {t("password.change.submit")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
