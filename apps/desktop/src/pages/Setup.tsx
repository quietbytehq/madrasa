import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { Check, Printer } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { ApiError, NetworkError } from "@/api/client";
import { api } from "@/api/endpoints";
import type { LocaleLists, SetupRequest } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { monthOptions } from "@/lib/months";
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
  /** ISO 3166-1 alpha-2. */
  country: string;
  /** 1–12 as a string (select value). */
  startMonth: string;
  /** ISO 4217. */
  currency: string;
  /** IANA name. */
  timeZone: string;
  /** Create the usual classes (KG–12 or K–12) with a section A each. */
  defaultClasses: boolean;
  adminFullName: string;
  adminUsername: string;
  adminPassword: string;
  adminPasswordConfirm: string;
  backupPassword: string;
  backupPasswordConfirm: string;
}

/** Enough to run setup when the lists cannot be fetched. */
export const FALLBACK_LISTS: LocaleLists = {
  countries: [
    {
      code: "PK",
      name: "Pakistan",
      time_zone: "Asia/Karachi",
      currency: "PKR",
      weekend: [7],
      id_label: "CNIC / B-Form",
      date_order: "day_first",
    },
    {
      code: "US",
      name: "United States",
      time_zone: "America/New_York",
      currency: "USD",
      weekend: [6, 7],
      id_label: "National ID",
      date_order: "month_first",
    },
  ],
  currencies: [
    { code: "PKR", name: "Pakistani rupee", symbol: "Rs" },
    { code: "USD", name: "US dollar", symbol: "$" },
  ],
  time_zones: ["Asia/Karachi", "America/New_York"],
};

/** Choosing a country sets its currency, time zone and the usual academic year start. */
export function applyCountry(f: SetupForm, code: string, lists: LocaleLists): SetupForm {
  const c = lists.countries.find((x) => x.code === code);
  if (!c) return { ...f, country: code };
  return {
    ...f,
    country: c.code,
    currency: c.currency,
    timeZone: c.time_zone,
    startMonth: code === "PK" ? "4" : code === "US" || code === "CA" ? "8" : f.startMonth,
  };
}

const EMPTY: SetupForm = {
  schoolName: "",
  schoolCode: "",
  city: "",
  country: "PK",
  startMonth: "4",
  currency: "PKR",
  timeZone: "Asia/Karachi",
  defaultClasses: true,
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
    academic_year_start_month: Number(f.startMonth),
    country: f.country,
    currency: f.currency,
    time_zone: f.timeZone,
    first_year: true,
    default_classes: f.defaultClasses,
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
  const [lists, setLists] = useState<LocaleLists>(FALLBACK_LISTS);
  useEffect(() => {
    let cancelled = false;
    api
      .localeLists()
      .then((l) => {
        if (!cancelled) setLists(l);
      })
      .catch(() => {
        // The fallback lists cover Pakistan and the US; the server validates the rest.
      });
    return () => {
      cancelled = true;
    };
  }, []);
  const { locale } = useI18n();
  const months = useMemo(() => monthOptions(locale), [locale]);

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
                    <Select
                      label={t("setup.school.country")}
                      hint={t("setup.school.countryHint")}
                      value={form.country}
                      onChange={(e) => setForm((f) => applyCountry(f, e.target.value, lists))}
                      options={lists.countries.map((c) => ({ value: c.code, label: c.name }))}
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
                      options={months}
                      className="max-w-xs"
                      autoFocus
                    />
                    <Select
                      label={t("setup.year.currency")}
                      value={form.currency}
                      onChange={set("currency")}
                      options={lists.currencies.map((c) => ({
                        value: c.code,
                        label: `${c.code} — ${c.name} (${c.symbol})`,
                      }))}
                      className="max-w-xs"
                    />
                    <Select
                      label={t("setup.year.timeZone")}
                      value={form.timeZone}
                      onChange={set("timeZone")}
                      options={lists.time_zones.map((z) => ({ value: z, label: z }))}
                      className="max-w-xs"
                    />
                    <label className="flex items-start gap-2 text-sm">
                      <input
                        type="checkbox"
                        className="mt-1"
                        checked={form.defaultClasses}
                        onChange={(e) =>
                          setForm((f) => ({ ...f, defaultClasses: e.target.checked }))
                        }
                      />
                      <span>
                        {t("setup.year.defaultClasses", {
                          first: form.country === "US" || form.country === "CA" ? "K" : "KG",
                        })}
                        <span className="block text-xs text-muted">
                          {t("setup.year.defaultClasses.hint")}
                        </span>
                      </span>
                    </label>
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
                          {months.find((m) => m.value === form.startMonth)?.label ??
                            form.startMonth}
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
