import { useState } from "react";
import type { FormEvent } from "react";
import { Check, Printer } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { ApiError, NetworkError } from "@/api/client";
import { api } from "@/api/endpoints";
import type { SetupRequest } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { useSetupGate } from "@/app/SetupGate";

const STEPS: TranslationKey[] = [
  "setup.step.school",
  "setup.step.year",
  "setup.step.admin",
  "setup.step.backup",
  "setup.step.done",
];

export interface SetupForm {
  schoolName: string;
  schoolCode: string;
  city: string;
  startMonth: "4" | "8";
  adminFullName: string;
  adminUsername: string;
  adminPassword: string;
  adminPasswordConfirm: string;
  backupPassword: string;
  backupPasswordConfirm: string;
}

const EMPTY: SetupForm = {
  schoolName: "",
  schoolCode: "",
  city: "",
  startMonth: "4",
  adminFullName: "",
  adminUsername: "",
  adminPassword: "",
  adminPasswordConfirm: "",
  backupPassword: "",
  backupPasswordConfirm: "",
};

type Errors = Partial<Record<keyof SetupForm, TranslationKey>>;

/** Per-step validation, mirrored by the backend (`crates/api/src/routes/setup.rs`). */
export function validateStep(step: number, f: SetupForm): Errors {
  const e: Errors = {};
  if (step === 0) {
    if (!f.schoolName.trim()) e.schoolName = "setup.error.required";
    if (!/^[A-Za-z0-9](?:[A-Za-z0-9-]{0,10}[A-Za-z0-9])?$/.test(f.schoolCode.trim()))
      e.schoolCode = "setup.error.codeFormat";
  } else if (step === 2) {
    if (!f.adminFullName.trim()) e.adminFullName = "setup.error.required";
    if (!f.adminUsername.trim()) e.adminUsername = "setup.error.required";
    if (f.adminPassword.length < 10) e.adminPassword = "setup.error.passwordShort";
    else if (f.adminPassword !== f.adminPasswordConfirm)
      e.adminPasswordConfirm = "setup.error.passwordMismatch";
  } else if (step === 3) {
    if (f.backupPassword.length < 10) e.backupPassword = "setup.error.passwordShort";
    else if (f.backupPassword !== f.backupPasswordConfirm)
      e.backupPasswordConfirm = "setup.error.passwordMismatch";
  }
  return e;
}

export function toRequest(f: SetupForm): SetupRequest {
  return {
    school_name: f.schoolName.trim(),
    school_code: f.schoolCode.trim().toUpperCase(),
    city: f.city.trim(),
    academic_year_start_month: f.startMonth === "8" ? 8 : 4,
    currency: "PKR",
    admin_full_name: f.adminFullName.trim(),
    admin_username: f.adminUsername.trim(),
    admin_password: f.adminPassword,
    backup_password: f.backupPassword,
  };
}

export function Setup() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const { markCompleted } = useSetupGate();
  const [step, setStep] = useState(0);
  const [form, setForm] = useState<SetupForm>(EMPTY);
  const [errors, setErrors] = useState<Errors>({});
  const [serverError, setServerError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null);
  const [acknowledged, setAcknowledged] = useState(false);

  const set = (key: keyof SetupForm) => (e: { target: { value: string } }) => {
    setForm((f) => ({ ...f, [key]: e.target.value }));
    setErrors((prev) => (prev[key] ? { ...prev, [key]: undefined } : prev));
  };

  const next = async (e: FormEvent) => {
    e.preventDefault();
    const errs = validateStep(step, form);
    setErrors(errs);
    if (Object.values(errs).some(Boolean)) return;
    if (step < 3) {
      setStep(step + 1);
      return;
    }
    setBusy(true);
    setServerError(null);
    try {
      const res = await api.setup(toRequest(form));
      setRecoveryKey(res.recovery_key);
      markCompleted();
      setStep(4);
    } catch (err) {
      if (err instanceof ApiError) setServerError(err.message);
      else if (err instanceof NetworkError) setServerError(t("app.error.network"));
      else setServerError(t("app.error.generic"));
    } finally {
      setBusy(false);
    }
  };

  const err = (key: keyof SetupForm) => (errors[key] ? t(errors[key]) : undefined);

  return (
    <main className="flex h-full items-start justify-center overflow-auto bg-bg p-6">
      <div className="w-full max-w-2xl">
        <div className="mb-4 flex items-center gap-2 print:hidden">
          <div
            aria-hidden
            className="flex h-8 w-8 items-center justify-center rounded bg-navy text-sm font-bold text-white"
          >
            M
          </div>
          <div>
            <h1 className="text-base font-semibold text-navy">{t("setup.title")}</h1>
            <p className="text-xs text-muted">{t("setup.intro")}</p>
          </div>
        </div>

        <ol
          className="mb-4 flex flex-wrap gap-2 text-xs print:hidden"
          aria-label={t("setup.title")}
        >
          {STEPS.map((key, i) => (
            <li
              key={key}
              aria-current={i === step ? "step" : undefined}
              className={cn(
                "flex items-center gap-1.5 rounded-full border px-2.5 py-1",
                i === step && "border-blue bg-selection text-blue",
                i < step && "border-border bg-card text-success",
                i > step && "border-border bg-card text-muted",
              )}
            >
              {i < step ? (
                <Check className="h-3 w-3" aria-hidden />
              ) : (
                <span className="tabular-nums">{i + 1}</span>
              )}
              {t(key)}
            </li>
          ))}
        </ol>

        {step < 4 ? (
          <form onSubmit={next} noValidate>
            <Card title={t(STEPS[step]!)}>
              <div className="flex flex-col gap-3">
                {step === 0 && (
                  <>
                    <Input
                      label={t("setup.school.name")}
                      value={form.schoolName}
                      onChange={set("schoolName")}
                      error={err("schoolName")}
                      autoFocus
                      required
                    />
                    <Input
                      label={t("setup.school.code")}
                      hint={t("setup.school.codeHint")}
                      value={form.schoolCode}
                      onChange={set("schoolCode")}
                      error={err("schoolCode")}
                      className="max-w-xs"
                      required
                    />
                    <Input
                      label={t("setup.school.city")}
                      labelSuffix={t("field.optional")}
                      value={form.city}
                      onChange={set("city")}
                      className="max-w-xs"
                    />
                  </>
                )}
                {step === 1 && (
                  <>
                    <Select
                      label={t("setup.year.startMonth")}
                      value={form.startMonth}
                      onChange={set("startMonth")}
                      options={[
                        { value: "4", label: t("setup.year.april") },
                        { value: "8", label: t("setup.year.august") },
                      ]}
                      className="max-w-xs"
                      autoFocus
                    />
                    <Input
                      label={t("setup.year.currency")}
                      value="PKR"
                      readOnly
                      className="max-w-xs"
                    />
                  </>
                )}
                {step === 2 && (
                  <>
                    <Input
                      label={t("setup.admin.fullName")}
                      value={form.adminFullName}
                      onChange={set("adminFullName")}
                      error={err("adminFullName")}
                      autoComplete="name"
                      autoFocus
                      required
                    />
                    <Input
                      label={t("setup.admin.username")}
                      value={form.adminUsername}
                      onChange={set("adminUsername")}
                      error={err("adminUsername")}
                      autoComplete="username"
                      className="max-w-xs"
                      required
                    />
                    <Input
                      label={t("setup.admin.password")}
                      type="password"
                      hint={t("setup.admin.passwordHint")}
                      value={form.adminPassword}
                      onChange={set("adminPassword")}
                      error={err("adminPassword")}
                      autoComplete="new-password"
                      className="max-w-xs"
                      required
                    />
                    <Input
                      label={t("setup.admin.passwordConfirm")}
                      type="password"
                      value={form.adminPasswordConfirm}
                      onChange={set("adminPasswordConfirm")}
                      error={err("adminPasswordConfirm")}
                      autoComplete="new-password"
                      className="max-w-xs"
                      required
                    />
                  </>
                )}
                {step === 3 && (
                  <>
                    <p className="text-xs text-muted">{t("setup.backup.hint")}</p>
                    <Input
                      label={t("setup.backup.password")}
                      type="password"
                      hint={t("setup.admin.passwordHint")}
                      value={form.backupPassword}
                      onChange={set("backupPassword")}
                      error={err("backupPassword")}
                      autoComplete="new-password"
                      className="max-w-xs"
                      autoFocus
                      required
                    />
                    <Input
                      label={t("setup.backup.passwordConfirm")}
                      type="password"
                      value={form.backupPasswordConfirm}
                      onChange={set("backupPasswordConfirm")}
                      error={err("backupPasswordConfirm")}
                      autoComplete="new-password"
                      className="max-w-xs"
                      required
                    />
                    <div className="mt-2 rounded border border-border bg-bg p-3 text-xs">
                      <div className="mb-1 font-semibold text-text">{t("setup.summary")}</div>
                      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 text-muted">
                        <dt>{t("setup.school.name")}</dt>
                        <dd className="text-text">{form.schoolName}</dd>
                        <dt>{t("setup.school.code")}</dt>
                        <dd className="text-text">{form.schoolCode.toUpperCase()}</dd>
                        <dt>{t("setup.year.startMonth")}</dt>
                        <dd className="text-text">
                          {form.startMonth === "8" ? t("setup.year.august") : t("setup.year.april")}
                        </dd>
                        <dt>{t("setup.admin.username")}</dt>
                        <dd className="text-text">{form.adminUsername}</dd>
                      </dl>
                    </div>
                  </>
                )}
                {serverError && (
                  <p role="alert" className="rounded bg-danger-bg px-2.5 py-2 text-xs text-danger">
                    {serverError}
                  </p>
                )}
              </div>
              <div className="mt-4 flex justify-between">
                <Button
                  type="button"
                  variant="secondary"
                  disabled={step === 0 || busy}
                  onClick={() => setStep(step - 1)}
                >
                  {t("action.back")}
                </Button>
                <Button type="submit" loading={busy}>
                  {step === 3 ? t("action.finish") : t("action.continue")}
                </Button>
              </div>
            </Card>
          </form>
        ) : (
          <Card
            title={t("setup.done.title")}
            actions={
              <Badge tone="success" className="print:hidden">
                {form.schoolCode.toUpperCase()}
              </Badge>
            }
          >
            <div className="print-area">
              <p className="mb-3 text-sm text-text">{t("setup.done.body")}</p>
              <div className="rounded-lg border-2 border-dashed border-navy/40 bg-bg p-4 text-center">
                <div className="mb-1 text-2xs uppercase tracking-wide text-muted">
                  {t("app.name")} · {form.schoolName} · {t("setup.step.done")}
                </div>
                <output
                  aria-label={t("setup.step.done")}
                  className="block select-all font-mono text-lg font-semibold tracking-widest text-navy"
                >
                  {recoveryKey}
                </output>
              </div>
            </div>
            <div className="mt-4 flex flex-col gap-3 print:hidden">
              <Button
                type="button"
                variant="secondary"
                icon={<Printer className="h-4 w-4" aria-hidden />}
                onClick={() => window.print()}
                className="self-start"
              >
                {t("action.print")}
              </Button>
              <label className="flex items-center gap-2 text-sm text-text">
                <input
                  type="checkbox"
                  checked={acknowledged}
                  onChange={(e) => setAcknowledged(e.target.checked)}
                  className="h-4 w-4 accent-blue"
                />
                {t("setup.done.ack")}
              </label>
              <Button
                type="button"
                disabled={!acknowledged}
                onClick={() => navigate("/login", { replace: true })}
                className="self-end"
              >
                {t("setup.done.go")}
              </Button>
            </div>
          </Card>
        )}
      </div>
    </main>
  );
}
