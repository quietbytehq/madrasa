import { useState } from "react";
import type { FormEvent } from "react";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/useAsync";
import { hub, setHubToken } from "../api";
import { LanguageSwitch } from "../LanguageSwitch";

export function LoginPage({
  hubName,
  onSignedIn,
}: {
  hubName: string | null;
  onSignedIn: () => void;
}) {
  const { t } = useI18n();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const { token } = await hub.login(username.trim(), password);
      setHubToken(token);
      onSignedIn();
    } catch (err) {
      setError(errorMessage(err, t("hub.error.network")));
      setBusy(false);
    }
  };

  return (
    <main className="flex min-h-screen items-center justify-center bg-bg p-4">
      <div className="w-full max-w-sm">
        <div className="mb-3 flex items-end justify-between gap-2">
          <div>
            <h1 className="text-lg font-semibold text-navy">{hubName ?? t("hub.name")}</h1>
            <p className="text-xs text-muted">{t("hub.tagline")}</p>
          </div>
          <LanguageSwitch />
        </div>
        <Card>
          <form onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-3">
            <Input
              label={t("hub.login.username")}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              autoComplete="username"
              autoFocus
              dir="ltr"
            />
            <Input
              label={t("hub.login.password")}
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="current-password"
            />
            {error && (
              <p role="alert" className="text-sm text-danger">
                {error}
              </p>
            )}
            <Button type="submit" loading={busy} disabled={!username.trim() || !password}>
              {t("action.login")}
            </Button>
          </form>
        </Card>
      </div>
    </main>
  );
}
