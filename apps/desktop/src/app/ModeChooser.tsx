import { useEffect, useState } from "react";
import { Monitor, Network, Server } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { useI18n } from "@/i18n";
import {
  DEFAULT_PORT,
  discoverServers,
  firewallCommand,
  probeServer,
  type FoundServer,
  type Mode,
  type ModeConfig,
  type ServerProbe,
} from "@/app/mode";

/**
 * Standalone, school server, or client of a school server: the three choices with their
 * details (port and firewall note; discovery, address and fingerprint confirmation). Used on
 * first start and under Settings → Network.
 */
export function ModeChooser({
  current,
  onSave,
}: {
  /** The mode this PC runs in now, if any. */
  current?: ModeConfig | null;
  onSave: (config: ModeConfig) => Promise<void>;
}) {
  const { t } = useI18n();
  const [choice, setChoice] = useState<Mode | null>(null);
  const [port, setPort] = useState(String(current?.listen_port ?? DEFAULT_PORT));
  const [address, setAddress] = useState("");
  const [found, setFound] = useState<FoundServer[] | null>(null);
  const [probe, setProbe] = useState<ServerProbe | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (choice !== "client" || found !== null) return;
    let cancelled = false;
    discoverServers()
      .then((list) => !cancelled && setFound(list))
      .catch(() => !cancelled && setFound([]));
    return () => {
      cancelled = true;
    };
  }, [choice, found]);

  const finish = async (config: ModeConfig) => {
    setBusy(true);
    setError(null);
    try {
      await onSave(config);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const check = async (url: string) => {
    setBusy(true);
    setError(null);
    setProbe(null);
    setConfirmed(false);
    try {
      setProbe(await probeServer(url));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-3 sm:grid-cols-3">
        {(
          [
            ["standalone", Monitor],
            ["server", Server],
            ["client", Network],
          ] as const
        ).map(([m, Icon]) => (
          <button
            key={m}
            type="button"
            aria-pressed={choice === m}
            onClick={() => {
              setChoice(m);
              setError(null);
            }}
            className={`flex flex-col items-start gap-1 rounded border p-3 text-start text-sm ${
              choice === m ? "border-blue bg-selection" : "border-border bg-card hover:bg-hover"
            }`}
          >
            <Icon className="h-5 w-5 text-blue" aria-hidden />
            <span className="font-semibold text-text">
              {t(`mode.${m}.title`)}
              {current?.mode === m ? ` · ${t("mode.current")}` : ""}
            </span>
            <span className="text-xs text-muted">{t(`mode.${m}.body`)}</span>
          </button>
        ))}
      </div>

      {choice === "standalone" && (
        <div>
          <Button
            loading={busy}
            onClick={() =>
              void finish({
                mode: "standalone",
                server_url: null,
                fingerprint: null,
                listen_port: DEFAULT_PORT,
              })
            }
          >
            {t("mode.standalone.use")}
          </Button>
        </div>
      )}

      {choice === "server" && (
        <Card title={t("mode.server.title")}>
          <div className="flex flex-col gap-3">
            <p className="text-sm text-muted">{t("mode.server.hint")}</p>
            <Input
              label={t("mode.server.port")}
              inputMode="numeric"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              className="max-w-xs"
            />
            <div className="rounded border border-warning/60 bg-warning-bg p-3 text-xs text-text">
              <div className="font-semibold">{t("mode.firewall.title")}</div>
              <p className="mt-1">{t("mode.firewall.body")}</p>
              <code className="mt-1 block break-all text-2xs">{firewallCommand()}</code>
            </div>
            <div>
              <Button
                loading={busy}
                onClick={() =>
                  void finish({
                    mode: "server",
                    server_url: null,
                    fingerprint: null,
                    listen_port: Number(port) || DEFAULT_PORT,
                  })
                }
              >
                {t("mode.server.start")}
              </Button>
            </div>
          </div>
        </Card>
      )}

      {choice === "client" && (
        <Card title={t("mode.client.title")}>
          <div className="flex flex-col gap-3">
            <p className="text-sm text-muted">{t("mode.client.hint")}</p>
            <div>
              <div className="mb-1 text-xs font-medium text-text">{t("mode.client.found")}</div>
              {found === null ? (
                <p className="text-xs text-muted">{t("mode.client.searching")}</p>
              ) : found.length === 0 ? (
                <p className="text-xs text-muted">{t("mode.client.none")}</p>
              ) : (
                <ul className="flex flex-col gap-1">
                  {found.map((f) => (
                    <li key={f.url}>
                      <Button size="sm" variant="secondary" onClick={() => void check(f.url)}>
                        {f.info.name} · {f.host}:{f.info.port}
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
              <Button size="sm" variant="ghost" className="mt-1" onClick={() => setFound(null)}>
                {t("mode.client.search")}
              </Button>
            </div>
            <div className="flex items-end gap-2">
              <Input
                label={t("mode.client.address")}
                hint={t("mode.client.addressHint")}
                value={address}
                onChange={(e) => setAddress(e.target.value)}
                className="max-w-xs"
              />
              <Button variant="secondary" loading={busy} onClick={() => void check(address)}>
                {t("mode.client.check")}
              </Button>
            </div>
            {probe && (
              <div className="rounded border border-border bg-bg p-3 text-sm">
                <div className="font-semibold text-text">{t("mode.client.fingerprintTitle")}</div>
                <p className="text-xs text-muted">{t("mode.client.fingerprintBody")}</p>
                <code
                  aria-label={t("mode.client.fingerprint")}
                  className="mt-1 block break-all text-xs"
                >
                  {probe.fingerprint}
                </code>
                <p className="mt-1 text-xs text-muted">
                  {probe.url} · Madrasa {probe.version}
                  {probe.needs_setup ? ` · ${t("mode.client.needsSetup")}` : ""}
                </p>
                <label className="mt-2 flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={confirmed}
                    onChange={(e) => setConfirmed(e.target.checked)}
                  />
                  {t("mode.client.confirm")}
                </label>
                <Button
                  className="mt-2"
                  loading={busy}
                  disabled={!confirmed}
                  onClick={() =>
                    void finish({
                      mode: "client",
                      server_url: probe.url,
                      fingerprint: probe.fingerprint,
                      listen_port: DEFAULT_PORT,
                    })
                  }
                >
                  {t("mode.client.connect")}
                </Button>
              </div>
            )}
          </div>
        </Card>
      )}
      {error && (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      )}
    </div>
  );
}
