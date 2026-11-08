import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { LibraryMemberKind, LibrarySettings, LoanRules } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

/** Loan rules per member kind and the fee head for student fines (library.manage). */
export function SettingsTab() {
  const { t } = useI18n();
  const toast = useToast();
  const settings = useAsync(() => api.librarySettings(), []);
  const heads = useAsync(() => api.feeHeads(), []);
  const [form, setForm] = useState<LibrarySettings | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (settings.data) setForm(settings.data);
  }, [settings.data]);
  if (!form) return <Spinner />;
  const rule = (kind: LibraryMemberKind, key: keyof LoanRules, label: string, step = "1") => (
    <Input
      key={`${kind}-${key}`}
      label={label}
      type="number"
      min={0}
      step={step}
      value={String(form.rules[kind][key])}
      onChange={(e) =>
        setForm({
          ...form,
          rules: {
            ...form.rules,
            [kind]: {
              ...form.rules[kind],
              [key]: key === "fine_per_day" ? e.target.value : Number(e.target.value) || 0,
            },
          },
        })
      }
      className="w-32"
    />
  );
  const save = async () => {
    setBusy(true);
    try {
      const body: LibrarySettings = {
        ...form,
        rules: {
          student: {
            ...form.rules.student,
            fine_per_day: Number(form.rules.student.fine_per_day) || 0,
          },
          staff: { ...form.rules.staff, fine_per_day: Number(form.rules.staff.fine_per_day) || 0 },
        },
      };
      settings.setData(await api.updateLibrarySettings(body));
      toast.push({ kind: "success", title: t("lib.settings.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      title={t("lib.tab.settings")}
      actions={
        <Button size="sm" loading={busy} onClick={() => void save()}>
          {t("action.save")}
        </Button>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("lib.settings.hint")}</p>
      {(["student", "staff"] as LibraryMemberKind[]).map((kind) => (
        <div key={kind} className="mb-3">
          <div className="mb-1 text-xs font-medium uppercase tracking-wide text-muted">
            {t(`lib.member.${kind}`)}
          </div>
          <div className="flex flex-wrap gap-3">
            {rule(kind, "max_books", t("lib.settings.maxBooks"))}
            {rule(kind, "loan_days", t("lib.settings.loanDays"))}
            {rule(kind, "fine_per_day", t("lib.settings.finePerDay"), "0.5")}
            {rule(kind, "max_renewals", t("lib.settings.renewals"))}
          </div>
        </div>
      ))}
      <Select
        label={t("lib.settings.feeHead")}
        value={form.fine_fee_head_id ?? ""}
        onChange={(e) => setForm({ ...form, fine_fee_head_id: e.target.value || null })}
        options={[
          { value: "", label: t("lib.settings.noFeeHead") },
          ...(heads.data ?? []).map((h) => ({ value: h.id, label: h.name })),
        ]}
        className="w-64"
      />
    </Card>
  );
}
