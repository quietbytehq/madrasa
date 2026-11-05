import { useEffect, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { ModeChooser } from "@/app/ModeChooser";
import {
  firewallCommand,
  getMode,
  networkInfo,
  setMode,
  type ModeConfig,
  type NetworkInfo,
} from "@/app/mode";

/**
 * Settings → Network: how this PC is used (just this PC, school server, client), how other
 * PCs reach a server (addresses, port, certificate fingerprint), and the switch between the
 * three. Only the desktop shell knows the mode, so the card is absent in the browser build.
 */
export function NetworkCard() {
  const { t } = useI18n();
  const [mode, setModeConfig] = useState<ModeConfig | null | undefined>(undefined);
  const [info, setInfo] = useState<NetworkInfo | null>(null);
  const [changing, setChanging] = useState(false);
  const [restarting, setRestarting] = useState(false);
  useEffect(() => {
    getMode().then(setModeConfig, () => setModeConfig(null));
    networkInfo().then(setInfo, () => setInfo(null));
  }, []);
  if (!mode) return null;

  const current = info?.mode ?? mode.mode;
  const fingerprint = info?.fingerprint ?? mode.fingerprint;
  const serverUrl = info?.server_url ?? mode.server_url;

  return (
    <Card
      title={t("network.title")}
      actions={
        !restarting && (
          <Button size="sm" variant="secondary" onClick={() => setChanging((c) => !c)}>
            {changing ? t("action.cancel") : t("network.change")}
          </Button>
        )
      }
    >
      <div className="flex flex-col gap-3">
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted">{t("network.mode")}</dt>
          <dd className="text-text">{t(`mode.${current}.title`)}</dd>
          {current === "server" && (
            <>
              <dt className="text-muted">{t("network.addresses")}</dt>
              <dd className="text-text">
                {info && info.addresses.length > 0 ? info.addresses.join(", ") : "—"}
              </dd>
              <dt className="text-muted">{t("network.port")}</dt>
              <dd className="text-text">{info?.port || mode.listen_port}</dd>
              <dt className="text-muted">{t("network.fingerprint")}</dt>
              <dd>
                <code className="break-all text-xs text-text">{fingerprint ?? "—"}</code>
              </dd>
              <dt className="text-muted">{t("mode.firewall.title")}</dt>
              <dd className="text-xs text-text">
                {t("mode.firewall.body")}
                <code className="mt-1 block break-all text-2xs">{firewallCommand()}</code>
              </dd>
            </>
          )}
          {current === "client" && (
            <>
              <dt className="text-muted">{t("network.server")}</dt>
              <dd className="text-text">{serverUrl}</dd>
              <dt className="text-muted">{t("network.fingerprint")}</dt>
              <dd>
                <code className="break-all text-xs text-text">{fingerprint ?? "—"}</code>
              </dd>
            </>
          )}
        </dl>
        {current === "standalone" && !changing && (
          <p className="text-xs text-muted">{t("network.standaloneHint")}</p>
        )}
        {restarting ? (
          <p className="flex items-center gap-2 text-sm text-text">
            <Spinner size="sm" />
            {t("network.restarting")}
          </p>
        ) : (
          changing && (
            <div className="border-t border-border pt-3">
              <p className="mb-3 text-xs text-muted">{t("network.changeHint")}</p>
              <ModeChooser
                current={mode}
                onSave={async (config) => {
                  await setMode(config);
                  setRestarting(true);
                }}
              />
            </div>
          )
        )}
      </div>
    </Card>
  );
}
