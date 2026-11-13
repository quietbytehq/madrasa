import { useState } from "react";
import { BrandMark } from "@/components/brand/Brand";
import type { FormEvent } from "react";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/useAsync";
import { hub } from "../api";
import { LanguageSwitch } from "../LanguageSwitch";

/** First run: name the network and create the first administrator. Works once. */
export function SetupPage({ onDone }: { onDone: () => void }) {
  const { t } = useI18n();
  const [f, setF] = useState({
    setup_token: "",
    hub_name: "",
    admin_full_name: "",
    admin_username: "",
    admin_password: "",
    confirm: "",
    escrow_passphrase: "",
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = (key: keyof typeof f) => (e: { target: { value: string } }) =>
    setF((prev) => ({ ...prev, [key]: e.target.value }));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (f.admin_password !== f.confirm) return setError(t("hub.setup.mismatch"));
    setBusy(true);
    setError(null);
    try {
      await hub.setup({
        setup_token: f.setup_token.trim(),
        hub_name: f.hub_name.trim(),
        admin_full_name: f.admin_full_name.trim(),
        admin_username: f.admin_username.trim(),
        admin_password: f.admin_password,
        ...(f.escrow_passphrase ? { escrow_passphrase: f.escrow_passphrase } : {}),
      });
      onDone();
    } catch (err) {
      setError(errorMessage(err, t("hub.error.network")));
      setBusy(false);
    }
  };

  const ready =
    f.setup_token.trim() &&
    f.hub_name.trim() &&
    f.admin_full_name.trim() &&
    f.admin_username.trim() &&
    f.admin_password &&
    f.confirm;

  return (
    <main className="flex min-h-screen items-center justify-center bg-bg p-4">
      <div className="w-full max-w-xl">
        <div className="mb-3 flex items-end justify-between gap-2">
          <div className="flex items-center gap-2">
            <BrandMark className="h-9 w-9" />
            <div>
              <h1 className="text-lg font-semibold text-navy">{t("hub.setup.title")}</h1>
              <p className="text-xs text-muted">{t("hub.setup.intro")}</p>
            </div>
          </div>
          <LanguageSwitch />
        </div>
        <Card>
          <form onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-3">
            <Input
              label={t("hub.setup.token")}
              hint={t("hub.setup.tokenHint")}
              value={f.setup_token}
              onChange={set("setup_token")}
              autoComplete="off"
              autoFocus
              dir="ltr"
            />
            <Input
              label={t("hub.setup.network")}
              hint={t("hub.setup.networkHint")}
              value={f.hub_name}
              onChange={set("hub_name")}
            />
            <div className="grid gap-3 sm:grid-cols-2">
              <Input
                label={t("hub.users.fullName")}
                value={f.admin_full_name}
                onChange={set("admin_full_name")}
                autoComplete="name"
              />
              <Input
                label={t("hub.login.username")}
                value={f.admin_username}
                onChange={set("admin_username")}
                autoComplete="username"
                dir="ltr"
              />
              <Input
                label={t("hub.login.password")}
                hint={t("hub.users.passwordHint")}
                type="password"
                value={f.admin_password}
                onChange={set("admin_password")}
                autoComplete="new-password"
              />
              <Input
                label={t("hub.setup.confirm")}
                type="password"
                value={f.confirm}
                onChange={set("confirm")}
                autoComplete="new-password"
              />
            </div>
            <Input
              label={t("hub.escrow.passphrase")}
              labelSuffix={t("field.optional")}
              hint={t("hub.setup.escrowHint")}
              type="password"
              value={f.escrow_passphrase}
              onChange={set("escrow_passphrase")}
              autoComplete="off"
            />
            {error && (
              <p role="alert" className="text-sm text-danger">
                {error}
              </p>
            )}
            <div>
              <Button type="submit" loading={busy} disabled={!ready}>
                {t("hub.setup.submit")}
              </Button>
            </div>
          </form>
        </Card>
      </div>
    </main>
  );
}
