import { useI18n } from "@/i18n";
import { ModeChooser } from "@/app/ModeChooser";
import { setMode, type ModeConfig } from "@/app/mode";

/** First start on a desktop PC: standalone, school server, or client of a school server. */
export function ModePage({ onChosen }: { onChosen: (config: ModeConfig) => void }) {
  const { t } = useI18n();
  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6">
      <h1 className="text-xl font-semibold text-navy">{t("mode.title")}</h1>
      <p className="text-sm text-muted">{t("mode.intro")}</p>
      <ModeChooser
        onSave={async (config) => {
          onChosen(await setMode(config));
        }}
      />
    </div>
  );
}
