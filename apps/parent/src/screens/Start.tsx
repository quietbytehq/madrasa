import { useState } from "react";
import { BrandMark } from "@/components/brand/Brand";
import type { FormEvent, ReactNode } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { useI18n } from "@/i18n";
import { hubApi, normalizeHubUrl } from "@parent/api";
import type { CodeCheck } from "@parent/api";
import { LanguageSwitch } from "@parent/LanguageSwitch";
import { useSession } from "@parent/session";
import { errorText } from "@parent/ui";

/** The frame of the screens before sign-in: the app's name, the language, one form. */
function StartFrame({ children }: { children: ReactNode }) {
  const { t } = useI18n();
  return (
    <div className="safe-top safe-bottom mx-auto flex min-h-full w-full max-w-md flex-col bg-bg px-4 py-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <BrandMark className="h-9 w-9" />
          <div className="leading-tight">
            <div className="font-display text-base font-black text-navy">
              {t("parent.app.name")}
            </div>
            <div className="text-xs text-muted">{t("parent.app.tagline")}</div>
          </div>
        </div>
        <LanguageSwitch />
      </div>
      <div className="mt-6 flex flex-1 flex-col">{children}</div>
    </div>
  );
}

function FormError({ children }: { children: ReactNode }) {
  return children ? (
    <p role="alert" className="rounded bg-danger-bg px-3 py-2 text-sm text-danger">
      {children}
    </p>
  ) : null;
}

/** First screen: two ways in. */
export function Welcome() {
  const { t } = useI18n();
  const { expired } = useSession();
  return (
    <StartFrame>
      <h1 className="text-xl font-semibold text-text">{t("parent.welcome.title")}</h1>
      <p className="mt-2 text-sm text-muted">{t("parent.welcome.body")}</p>
      {expired && (
        <p role="status" className="mt-4 rounded bg-warning-bg px-3 py-2 text-sm text-text">
          {t("parent.error.signedOut")}
        </p>
      )}
      <div className="mt-6 flex flex-col gap-3">
        <Link
          to="/code"
          className="flex min-h-12 items-center justify-center rounded bg-blue px-4 text-sm font-medium text-white hover:bg-blue/90"
        >
          {t("parent.welcome.haveCode")}
        </Link>
        <Link
          to="/login"
          className="flex min-h-12 items-center justify-center rounded border border-border bg-card px-4 text-sm font-medium text-text hover:bg-hover"
        >
          {t("parent.welcome.signIn")}
        </Link>
      </div>
      <p className="mt-6 text-xs text-muted">{t("parent.welcome.noCode")}</p>
    </StartFrame>
  );
}

/** Sign in with the phone number and password chosen when the code was used. */
export function Login() {
  const { t } = useI18n();
  const session = useSession();
  const navigate = useNavigate();
  const [hub, setHub] = useState(session.hubUrl ?? "");
  const [phone, setPhone] = useState(session.phone ?? "");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const address = normalizeHubUrl(hub);
    if (!address) return setProblem(t("parent.hub.invalid"));
    setBusy(true);
    setProblem(null);
    try {
      session.signIn(address, await hubApi.login(address, phone.trim(), password));
      navigate("/", { replace: true });
    } catch (err) {
      setProblem(errorText(err, t));
    } finally {
      setBusy(false);
    }
  };

  return (
    <StartFrame>
      <h1 className="text-xl font-semibold text-text">{t("parent.login.title")}</h1>
      <form className="mt-4 flex flex-col gap-3" onSubmit={(e) => void submit(e)} noValidate>
        <Input
          label={t("parent.hub.label")}
          hint={t("parent.hub.hint")}
          value={hub}
          onChange={(e) => setHub(e.target.value)}
          inputMode="url"
          autoCapitalize="none"
          autoCorrect="off"
          dir="ltr"
          required
          data-testid="hub-address"
        />
        <Input
          label={t("parent.phone.label")}
          value={phone}
          onChange={(e) => setPhone(e.target.value)}
          type="tel"
          autoComplete="username"
          dir="ltr"
          required
          data-testid="phone"
        />
        <Input
          label={t("parent.password.label")}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          type="password"
          autoComplete="current-password"
          dir="ltr"
          required
          data-testid="password"
        />
        <FormError>{problem}</FormError>
        <Button
          type="submit"
          loading={busy}
          disabled={!hub.trim() || !phone.trim() || !password}
          className="min-h-12"
          data-testid="sign-in"
        >
          {t("parent.login.submit")}
        </Button>
      </form>
      <p className="mt-4 text-xs text-muted">{t("parent.login.forgot")}</p>
      <Link to="/code" className="mt-4 text-sm font-medium text-blue underline">
        {t("parent.welcome.haveCode")}
      </Link>
    </StartFrame>
  );
}

/**
 * Use an access code from the school's slip. Step one asks the hub whose code it is, so
 * the parent sees their own name and their children before choosing a password. The same
 * screen adds another school to an account and sets a new password after a forgotten one:
 * which of the three happens is decided by the phone number, at the hub.
 */
export function UseCode() {
  const { t } = useI18n();
  const session = useSession();
  const navigate = useNavigate();
  const [hub, setHub] = useState(session.hubUrl ?? "");
  const [code, setCode] = useState("");
  const [checked, setChecked] = useState<{ hub: string; info: CodeCheck } | null>(null);
  const [phone, setPhone] = useState(session.phone ?? "");
  const [password, setPassword] = useState("");
  const [again, setAgain] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const check = async (e: FormEvent) => {
    e.preventDefault();
    const address = normalizeHubUrl(hub);
    if (!address) return setProblem(t("parent.hub.invalid"));
    setBusy(true);
    setProblem(null);
    try {
      setChecked({ hub: address, info: await hubApi.checkCode(address, code.trim()) });
    } catch (err) {
      setProblem(errorText(err, t));
    } finally {
      setBusy(false);
    }
  };

  const activate = async (e: FormEvent) => {
    e.preventDefault();
    if (!checked) return;
    // Someone adding a school to their account gives its password once; nothing to repeat.
    if (!session.signedIn && password !== again) return setProblem(t("parent.password.mismatch"));
    setBusy(true);
    setProblem(null);
    try {
      const opened = await hubApi.activate(checked.hub, code.trim(), phone.trim(), password);
      session.signIn(checked.hub, opened);
      navigate("/", { replace: true });
    } catch (err) {
      setProblem(errorText(err, t));
    } finally {
      setBusy(false);
    }
  };

  if (!checked) {
    return (
      <StartFrame>
        <h1 className="text-xl font-semibold text-text">{t("parent.code.title")}</h1>
        <p className="mt-2 text-sm text-muted">{t("parent.code.body")}</p>
        <form className="mt-4 flex flex-col gap-3" onSubmit={(e) => void check(e)} noValidate>
          <Input
            label={t("parent.hub.label")}
            hint={t("parent.hub.hint")}
            value={hub}
            onChange={(e) => setHub(e.target.value)}
            inputMode="url"
            autoCapitalize="none"
            autoCorrect="off"
            dir="ltr"
            required
            data-testid="hub-address"
          />
          <Input
            label={t("parent.code.label")}
            hint={t("parent.code.hint")}
            value={code}
            onChange={(e) => setCode(e.target.value.toUpperCase())}
            autoCapitalize="characters"
            autoCorrect="off"
            dir="ltr"
            className="font-mono tracking-widest"
            required
            data-testid="access-code"
          />
          <FormError>{problem}</FormError>
          <Button
            type="submit"
            loading={busy}
            disabled={!hub.trim() || !code.trim()}
            className="min-h-12"
            data-testid="check-code"
          >
            {t("action.continue")}
          </Button>
        </form>
        <Link
          to={session.signedIn ? "/" : "/login"}
          className="mt-4 text-sm font-medium text-blue underline"
        >
          {session.signedIn ? t("action.back") : t("parent.code.haveAccount")}
        </Link>
      </StartFrame>
    );
  }

  const { info } = checked;
  return (
    <StartFrame>
      <h1 className="text-xl font-semibold text-text">
        {t("parent.code.greeting", { name: info.guardian_name })}
      </h1>
      <div
        className="mt-3 rounded-lg border border-border bg-card p-3 text-sm"
        data-testid="code-for"
      >
        <div className="text-muted">{info.school_name}</div>
        <div className="mt-1 font-medium text-text">{t("parent.code.children")}</div>
        <ul className="list-disc ps-5 text-text">
          {info.children.map((name) => (
            <li key={name}>{name}</li>
          ))}
        </ul>
      </div>
      <p className="mt-3 text-sm text-muted">
        {session.signedIn
          ? t("parent.code.addToAccount")
          : info.existing_phone_hint
            ? t("parent.code.existing", { hint: info.existing_phone_hint })
            : t("parent.code.choose")}
      </p>
      <form className="mt-3 flex flex-col gap-3" onSubmit={(e) => void activate(e)} noValidate>
        <Input
          label={t("parent.phone.label")}
          hint={t("parent.phone.hint")}
          value={phone}
          onChange={(e) => setPhone(e.target.value)}
          type="tel"
          autoComplete="username"
          dir="ltr"
          required
          data-testid="phone"
        />
        <Input
          label={session.signedIn ? t("parent.password.current") : t("parent.password.new")}
          hint={session.signedIn ? undefined : t("parent.password.hint")}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          type="password"
          autoComplete={session.signedIn ? "current-password" : "new-password"}
          dir="ltr"
          required
          data-testid="password"
        />
        {!session.signedIn && (
          <Input
            label={t("parent.password.again")}
            value={again}
            onChange={(e) => setAgain(e.target.value)}
            type="password"
            autoComplete="new-password"
            dir="ltr"
            required
            data-testid="password-again"
          />
        )}
        <FormError>{problem}</FormError>
        <Button
          type="submit"
          loading={busy}
          disabled={!phone.trim() || !password}
          className="min-h-12"
          data-testid="activate"
        >
          {t("parent.code.submit")}
        </Button>
      </form>
      <button
        type="button"
        onClick={() => {
          setChecked(null);
          setProblem(null);
        }}
        className="mt-4 self-start text-sm font-medium text-blue underline"
      >
        {t("parent.code.other")}
      </button>
    </StartFrame>
  );
}
