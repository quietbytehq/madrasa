import { useEffect, useState } from "react";
import { Card } from "@/components/ui/Card";
import { useI18n } from "@/i18n";
import { firewallCommand, networkInfo, type NetworkInfo } from "@/app/mode";

/** Settings → Network: how other PCs reach this server, or which server this PC uses. */
export function NetworkCard() {
  const { t } = useI18n();
  const [info, setInfo] = useState<NetworkInfo | null | undefined>(undefined);
  useEffect(() => {
    networkInfo().then(setInfo, () => setInfo(null));
  }, []);
  if (!info || info.mode === "standalone") return null;
  return (
    <Card title={t("network.title")}>
      {info.mode === "server" ? (
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted">{t("network.addresses")}</dt>
          <dd className="text-text">
            {info.addresses.length > 0 ? info.addresses.join(", ") : "—"}
          </dd>
          <dt className="text-muted">{t("network.port")}</dt>
          <dd className="text-text">{info.port}</dd>
          <dt className="text-muted">{t("network.fingerprint")}</dt>
          <dd>
            <code className="break-all text-xs text-text">{info.fingerprint}</code>
          </dd>
          <dt className="text-muted">{t("mode.firewall.title")}</dt>
          <dd className="text-xs text-text">
            {t("mode.firewall.body")}
            <code className="mt-1 block break-all text-2xs">{firewallCommand()}</code>
          </dd>
        </dl>
      ) : (
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted">{t("network.server")}</dt>
          <dd className="text-text">{info.server_url}</dd>
          <dt className="text-muted">{t("network.fingerprint")}</dt>
          <dd>
            <code className="break-all text-xs text-text">{info.fingerprint}</code>
          </dd>
        </dl>
      )}
    </Card>
  );
}
