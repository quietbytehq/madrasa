import { useEffect, useState } from "react";
import type { ChangeEvent, FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { SchoolProfileUpdate } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { AcademicsSettings } from "./academics/AcademicsSettings";
import { PageHeader } from "./PageHeader";

export function SettingsPage() {
  const { t } = useI18n();
  const { can } = useSession();
  return (
    <>
      <PageHeader
        title={t("settings.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("settings.title") }]}
      />
      <div className="flex flex-col gap-4">
        {can("settings.manage") && <SchoolSettingsCard />}
        {can("settings.manage") && <SchoolProfileCard />}
        <section>
          <h2 className="mb-2 text-sm font-semibold text-navy">{t("academics.title")}</h2>
          <AcademicsSettings />
        </section>
      </div>
    </>
  );
}

function SchoolSettingsCard() {
  const { t } = useI18n();
  const toast = useToast();
  const settings = useAsync(() => api.schoolSettings(), []);
  const [prefix, setPrefix] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (settings.data) setPrefix(settings.data.student_number_prefix);
  }, [settings.data]);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const res = await api.updateSchoolSettings({ student_number_prefix: prefix });
      settings.setData(res);
      toast.push({ kind: "success", title: t("common.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };

  const example = `${prefix || "KHI-MC"}-${new Date().getFullYear()}-00417`;
  return (
    <Card title={t("settings.school")}>
      <form onSubmit={save} className="flex items-end gap-2">
        <Input
          label={t("settings.studentNumberPrefix")}
          hint={t("settings.studentNumberPrefix.hint", { example })}
          value={prefix}
          onChange={(e) => setPrefix(e.target.value)}
          className="max-w-xs"
          required
        />
        <Button type="submit" loading={busy}>
          {t("action.save")}
        </Button>
      </form>
    </Card>
  );
}

function SchoolProfileCard() {
  const { t } = useI18n();
  const toast = useToast();
  const { school, refreshSchool } = useSession();
  const lists = useAsync(() => api.localeLists(), []);
  const [form, setForm] = useState<SchoolProfileUpdate>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setForm({
      name: school.name,
      city: school.city,
      country: school.country,
      currency: school.currency,
      time_zone: school.time_zone,
    });
  }, [school]);
  const set =
    (key: keyof SchoolProfileUpdate) => (e: ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
      setForm((f) => ({ ...f, [key]: e.target.value }));
  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api.updateSchool(form);
      await refreshSchool();
      toast.push({ kind: "success", title: t("common.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const countries = lists.data?.countries ?? [];
  const currencies = lists.data?.currencies ?? [];
  const zones = lists.data?.time_zones ?? [];
  return (
    <Card title={t("settings.profile")}>
      <form onSubmit={save} className="flex flex-col gap-3">
        <p className="text-xs text-muted">{t("settings.profile.hint")}</p>
        <div className="grid gap-3 sm:grid-cols-2">
          <Input
            label={t("settings.profile.name")}
            value={form.name ?? ""}
            onChange={set("name")}
            required
          />
          <Input
            label={t("settings.profile.city")}
            value={form.city ?? ""}
            onChange={set("city")}
          />
          <Select
            label={t("settings.profile.country")}
            value={form.country ?? ""}
            onChange={set("country")}
            options={countries.map((c) => ({ value: c.code, label: c.name }))}
          />
          <Select
            label={t("settings.profile.currency")}
            value={form.currency ?? ""}
            onChange={set("currency")}
            options={currencies.map((c) => ({
              value: c.code,
              label: `${c.code} — ${c.name} (${c.symbol})`,
            }))}
          />
          <Select
            label={t("settings.profile.timeZone")}
            value={form.time_zone ?? ""}
            onChange={set("time_zone")}
            options={zones.map((z) => ({ value: z, label: z }))}
          />
        </div>
        <div>
          <Button type="submit" loading={busy}>
            {t("action.save")}
          </Button>
        </div>
      </form>
    </Card>
  );
}
