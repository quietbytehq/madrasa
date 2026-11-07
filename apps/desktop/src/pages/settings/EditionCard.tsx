import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { Edition, Feature } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useFeatures } from "@/app/features";
import { errorMessage, useAsync } from "@/lib/useAsync";

const FEATURE_KEYS: Partial<Record<Feature, TranslationKey>> = {
  timetable: "feature.timetable",
  leave: "feature.leave",
  admissions: "feature.admissions",
  library: "feature.library",
  transport: "feature.transport",
  hostel: "feature.hostel",
  inventory: "feature.inventory",
  hr: "feature.hr",
  campuses: "feature.campuses",
  directory_login: "feature.directory_login",
  readonly_api: "feature.readonly_api",
};

/** Settings → Edition: switch an instance between Lite and Standard (Standard alpha, stage 0). */
export function EditionCard() {
  const { t } = useI18n();
  const toast = useToast();
  const features = useFeatures();
  const settings = useAsync(() => api.editionSettings(), []);
  const [choice, setChoice] = useState<Edition>("lite");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (settings.data) setChoice(settings.data.edition);
  }, [settings.data]);
  if (settings.loading && !settings.data)
    return (
      <Card title={t("settings.edition")}>
        <Spinner />
      </Card>
    );
  const d = settings.data;
  const fixed = d?.edition === "nexus";
  const save = async () => {
    setBusy(true);
    try {
      const next = await api.updateEdition(choice);
      settings.setData(next);
      await features.reload();
      toast.push({ kind: "success", title: t("settings.edition.switched") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title={t("settings.edition")}>
      <div className="flex flex-col gap-3 text-sm">
        <p className="text-xs text-muted">{t("settings.edition.hint")}</p>
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-muted">{t("settings.edition.current")}:</span>
          <Badge tone="info" data-testid="edition-current">
            {d ? t(`settings.edition.${d.edition}`) : "—"}
          </Badge>
        </div>
        <div>
          <div className="mb-1 text-xs font-medium text-muted">{t("settings.edition.adds")}</div>
          <div className="flex flex-wrap gap-1">
            {(d?.standard_only ?? []).map((f) => (
              <Badge key={f} tone="neutral">
                {FEATURE_KEYS[f] ? t(FEATURE_KEYS[f]!) : f}
              </Badge>
            ))}
          </div>
        </div>
        {fixed ? (
          <p className="text-xs text-muted">{t("settings.edition.nexusFixed")}</p>
        ) : (
          <div className="flex flex-wrap items-end gap-2">
            <Select
              label={t("settings.edition")}
              value={choice}
              onChange={(e) => setChoice(e.target.value as Edition)}
              options={(d?.available ?? ["lite", "standard"]).map((e) => ({
                value: e,
                label: t(`settings.edition.${e}`),
              }))}
              className="w-48"
            />
            <Button
              loading={busy}
              disabled={!d || choice === d.edition}
              onClick={() => void save()}
            >
              {t("settings.edition.switch")}
            </Button>
          </div>
        )}
      </div>
    </Card>
  );
}
