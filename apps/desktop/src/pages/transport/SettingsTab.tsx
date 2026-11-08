import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

/** The fee head that carries the monthly transport charge on vouchers (transport.manage). */
export function SettingsTab() {
  const { t } = useI18n();
  const toast = useToast();
  const settings = useAsync(() => api.transportSettings(), []);
  const heads = useAsync(() => api.feeHeads(), []);
  const [head, setHead] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (settings.data) setHead(settings.data.fee_head_id ?? "");
  }, [settings.data]);
  if (head === null) return <Spinner />;
  const save = async () => {
    setBusy(true);
    try {
      settings.setData(await api.updateTransportSettings({ fee_head_id: head || null }));
      toast.push({ kind: "success", title: t("trp.settings.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      title={t("trp.tab.settings")}
      actions={
        <Button size="sm" loading={busy} onClick={() => void save()}>
          {t("action.save")}
        </Button>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("trp.settings.hint")}</p>
      <Select
        label={t("trp.settings.feeHead")}
        value={head}
        onChange={(e) => setHead(e.target.value)}
        options={[
          { value: "", label: t("trp.settings.noFeeHead") },
          ...(heads.data ?? []).map((h) => ({ value: h.id, label: h.name })),
        ]}
        className="w-64"
      />
    </Card>
  );
}
