import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
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
