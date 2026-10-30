import { useState } from "react";
import type { FormEvent } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { ApiError, NetworkError } from "@/api/client";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { useI18n } from "@/i18n";
import { useSession } from "@/app/session";

export function Login() {
  const { t } = useI18n();
  const { login } = useSession();
  const navigate = useNavigate();
  const location = useLocation();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await login({ username: username.trim(), password });
      const from = (location.state as { from?: string } | null)?.from ?? "/";
      navigate(from, { replace: true });
    } catch (err) {
      if (err instanceof ApiError) {
        if (err.code === "account_locked") setError(t("login.locked"));
        else if (err.code === "account_inactive") setError(t("login.inactive"));
        else if (err.status === 401) setError(t("login.failed"));
        else setError(err.message || t("app.error.generic"));
      } else if (err instanceof NetworkError) {
        setError(t("app.error.network"));
      } else {
        setError(t("app.error.generic"));
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="flex h-full items-center justify-center bg-bg p-4">
      <Card className="w-full max-w-sm">
        <div className="mb-4 flex items-center gap-2">
          <div
            aria-hidden
            className="flex h-8 w-8 items-center justify-center rounded bg-navy text-sm font-bold text-white"
          >
            M
          </div>
          <div>
            <h1 className="text-base font-semibold text-navy">{t("login.title")}</h1>
            <p className="text-xs text-muted">{t("app.tagline")}</p>
          </div>
        </div>
        <form onSubmit={onSubmit} className="flex flex-col gap-3" noValidate>
          <Input
            label={t("login.username")}
            name="username"
            autoComplete="username"
            autoFocus
            required
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
          <Input
            label={t("login.password")}
            name="password"
            type="password"
            autoComplete="current-password"
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          {error && (
            <p role="alert" className="rounded bg-danger-bg px-2.5 py-2 text-xs text-danger">
              {error}
            </p>
          )}
          <Button type="submit" loading={busy} className="mt-1">
            {t("login.submit")}
          </Button>
        </form>
      </Card>
    </main>
  );
}
