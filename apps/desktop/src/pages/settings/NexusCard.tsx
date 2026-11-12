import { useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { NexusLinkStatus, NexusOverwrite } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { announceParentAppChange } from "@/app/parentApp";
import { showValue } from "@/lib/showValue";
import { NexusEscrow } from "./NexusEscrow";

const REASONS: Record<string, TranslationKey> = {
  head_office_wins: "nexus.reason.head_office_wins",
  later_mark: "nexus.reason.later_mark",
  renumbered: "nexus.reason.renumbered",
  duplicate_reversal: "nexus.reason.duplicate_reversal",
};

/**
 * Settings → Nexus: join a school network (a Madrasa Nexus hub), see how syncing is going,
 * see what head office overwrote, and leave. A school that has not joined is unchanged.
 */
export function NexusCard({ canManageBackups = false }: { canManageBackups?: boolean }) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const link = useAsync(() => api.nexusLink(), []);
  const [hubUrl, setHubUrl] = useState("");
  const [joinCode, setJoinCode] = useState("");
  const [busy, setBusy] = useState<"join" | "sync" | "leave" | null>(null);
  const [leaving, setLeaving] = useState(false);
  const [reason, setReason] = useState("");
  const [log, setLog] = useState<NexusOverwrite[] | null>(null);

  const when = (value: string | null) =>
    value ? formatDate(value, { dateStyle: "medium", timeStyle: "short" }) : t("nexus.never");

  const loadLog = async () => {
    try {
      setLog(await api.nexusOverwrites());
    } catch {
      setLog([]);
    }
  };

  const syncNow = async (announce: boolean) => {
    setBusy("sync");
    try {
      const next = await api.nexusSync();
      link.setData(next);
      if (next.overwrites > 0) await loadLog();
      if (announce) {
        toast.push(
          next.last_error
            ? { kind: "error", title: next.last_error }
            : { kind: "success", title: t("nexus.synced") },
        );
      }
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };

  const join = async (e: FormEvent) => {
    e.preventDefault();
    setBusy("join");
    try {
      const next = await api.nexusJoin(hubUrl.trim(), joinCode.trim());
      link.setData(next);
      announceParentAppChange();
      setJoinCode("");
      toast.push({ kind: "success", title: t("nexus.joined", { hub: next.hub_name ?? "" }) });
      // The first upload starts at once; the screen shows it finishing.
      await syncNow(false);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
      setBusy(null);
    }
  };

  const leave = async () => {
    setBusy("leave");
    try {
      const out = await api.nexusLeave(reason.trim());
      link.setData(out.status);
      announceParentAppChange();
      setLeaving(false);
      setReason("");
      setLog(null);
      toast.push({
        kind: out.hub_told ? "success" : "error",
        title: out.hub_told ? t("nexus.left") : t("nexus.leftUntold"),
      });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };

  if (link.loading && !link.data)
    return (
      <Card title={t("nexus.title")}>
        <Spinner />
      </Card>
    );
  const d: NexusLinkStatus | null = link.data;
  if (!d) return null;

  if (!d.linked)
    return (
      <Card title={t("nexus.title")}>
        <form onSubmit={(e) => void join(e)} className="flex flex-col gap-3 text-sm">
          <p className="text-xs text-muted">{t("nexus.hint")}</p>
          {d.ended && (
            <p
              role="status"
              className="rounded border border-border bg-hover p-2 text-xs text-text"
            >
              {d.ended.by === "head_office"
                ? t("nexus.endedByHub", { date: when(d.ended.at), reason: d.ended.reason })
                : t("nexus.endedBySchool", { hub: d.hub_name ?? "", date: when(d.ended.at) })}
            </p>
          )}
          <div className="grid gap-3 sm:grid-cols-2">
            <Input
              label={t("nexus.hubUrl")}
              hint={t("nexus.hubUrlHint")}
              placeholder="https://nexus.example.org"
              value={hubUrl}
              onChange={(e) => setHubUrl(e.target.value)}
              required
              dir="ltr"
            />
            <Input
              label={t("nexus.joinCode")}
              hint={t("nexus.joinCodeHint")}
              value={joinCode}
              onChange={(e) => setJoinCode(e.target.value)}
              required
              autoComplete="off"
              dir="ltr"
            />
          </div>
          <p className="text-xs text-muted">{t("nexus.privacy")}</p>
          <div>
            <Button
              type="submit"
              loading={busy === "join" || busy === "sync"}
              disabled={!hubUrl.trim() || !joinCode.trim()}
            >
              {t("nexus.join")}
            </Button>
          </div>
        </form>
      </Card>
    );

  return (
    <Card
      title={t("nexus.title")}
      actions={
        <Badge tone={d.last_error ? "warning" : "success"} data-testid="nexus-state">
          {d.last_error ? t("nexus.state.problem") : t("nexus.state.joined")}
        </Badge>
      }
    >
      <div className="flex flex-col gap-3 text-sm">
        <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1">
          <dt className="text-muted">{t("nexus.network")}</dt>
          <dd className="font-medium">{d.hub_name}</dd>
          <dt className="text-muted">{t("nexus.hubUrl")}</dt>
          <dd dir="ltr" className="text-start">
            {d.hub_url}
          </dd>
          <dt className="text-muted">{t("nexus.joinedAt")}</dt>
          <dd>{when(d.joined_at)}</dd>
          <dt className="text-muted">{t("nexus.lastSync")}</dt>
          <dd data-testid="nexus-last-sync">{when(d.last_sync_at)}</dd>
          <dt className="text-muted">{t("nexus.pending")}</dt>
          <dd data-testid="nexus-pending">{d.pending}</dd>
        </dl>
        {d.last_error && (
          <p
            role="alert"
            className="rounded border border-warning/60 bg-warning-bg p-2 text-xs text-text"
          >
            {d.last_error}
          </p>
        )}
        {d.parked.length > 0 && (
          <div className="rounded border border-warning/60 bg-warning-bg p-2 text-xs text-text">
            <div className="font-semibold">{t("nexus.parked", { count: d.parked.length })}</div>
            <ul className="mt-1 list-disc ps-5">
              {d.parked.slice(0, 5).map((p) => (
                <li key={`${p.table}:${p.id}`}>
                  <span dir="ltr">{p.table}</span>: {p.reason}
                </li>
              ))}
            </ul>
          </div>
        )}
        <div className="flex flex-wrap gap-2">
          <Button loading={busy === "sync"} onClick={() => void syncNow(true)}>
            {t("nexus.syncNow")}
          </Button>
          <Button variant="secondary" onClick={() => setLeaving(true)}>
            {t("nexus.leave")}
          </Button>
        </div>

        <div>
          <div className="mb-1 flex items-center justify-between gap-2">
            <h3 className="text-xs font-semibold text-navy">
              {t("nexus.log.title", { count: d.overwrites })}
            </h3>
            {d.overwrites > 0 && log === null && (
              <Button variant="ghost" size="sm" onClick={() => void loadLog()}>
                {t("nexus.log.show")}
              </Button>
            )}
          </div>
          <p className="text-xs text-muted">{t("nexus.log.hint")}</p>
          {log && log.length > 0 && (
            <div className="mt-2 overflow-x-auto">
              <table className="w-full text-start text-xs">
                <thead className="text-muted">
                  <tr>
                    <th className="py-1 pe-3 text-start font-medium">{t("nexus.log.when")}</th>
                    <th className="py-1 pe-3 text-start font-medium">{t("nexus.log.record")}</th>
                    <th className="py-1 pe-3 text-start font-medium">{t("nexus.log.kept")}</th>
                    <th className="py-1 pe-3 text-start font-medium">{t("nexus.log.replaced")}</th>
                    <th className="py-1 text-start font-medium">{t("nexus.log.why")}</th>
                  </tr>
                </thead>
                <tbody>
                  {log.map((o, i) => (
                    <tr key={o.uid ?? i} className="border-t border-border align-top">
                      <td className="whitespace-nowrap py-1 pe-3">{when(o.at)}</td>
                      <td className="py-1 pe-3" dir="ltr">
                        {o.table}.{o.field}
                      </td>
                      <td className="py-1 pe-3">{showValue(o.kept)}</td>
                      <td className="py-1 pe-3">{showValue(o.discarded)}</td>
                      <td className="py-1">
                        {REASONS[o.reason] ? t(REASONS[o.reason]!) : o.reason}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        <NexusEscrow canManageBackups={canManageBackups} />
      </div>

      <Dialog
        open={leaving}
        onClose={() => setLeaving(false)}
        title={t("nexus.leave.title", { hub: d.hub_name ?? "" })}
        description={t("nexus.leave.body")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setLeaving(false)}>
              {t("action.cancel")}
            </Button>
            <Button variant="danger" loading={busy === "leave"} onClick={() => void leave()}>
              {t("nexus.leave.confirm")}
            </Button>
          </>
        }
      >
        <Input
          label={t("nexus.leave.reason")}
          labelSuffix={t("field.optional")}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
      </Dialog>
    </Card>
  );
}
