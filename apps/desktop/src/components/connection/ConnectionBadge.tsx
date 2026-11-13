import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { outboxDismiss, outboxRetry } from "@/api/client";
import type { OutboxLink, OutboxStatus } from "@/api/types";
import { useConnection } from "@/app/connection";
import { useSession } from "@/app/session";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { describeChange, hubSummary } from "@/lib/connection";
import { deleteDraft, listDrafts } from "@/lib/drafts";
import type { Draft } from "@/lib/drafts";

const DOT: Record<OutboxLink, string> = {
  connected: "bg-success",
  reconnecting: "bg-warning",
  offline: "bg-danger",
};

const FIELDS: Record<string, TranslationKey> = {
  status: "offline.field.status",
  address: "offline.field.address",
  full_name: "offline.field.name",
  father_name: "offline.field.father",
  phone: "offline.field.phone",
  date_of_birth: "offline.field.dob",
};

function show(v: unknown): string {
  if (v === null || v === undefined || v === "") return "—";
  return typeof v === "string" ? v : JSON.stringify(v);
}

/** Always in the top bar: is the school server there, what is waiting, and the hub's sync. */
export function ConnectionBadge() {
  const { t } = useI18n();
  const connection = useConnection();
  const [open, setOpen] = useState(false);
  if (!connection) return null;
  const { link, outbox, hub } = connection;
  const waiting = outbox?.waiting ?? 0;
  const attention =
    (outbox?.failed.length ?? 0) +
    (outbox?.overwritten.length ?? 0) +
    (outbox?.notices.length ?? 0);
  const hubText = hubSummary(t, hub);
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="flex h-8 items-center gap-1.5 rounded border border-border px-2 text-xs text-text hover:bg-hover"
        data-testid="connection-badge"
        aria-label={t("offline.badge.label")}
      >
        <span className={cn("h-2 w-2 shrink-0 rounded-full", DOT[link])} aria-hidden />
        <span data-testid="connection-link">{t(`offline.link.${link}`)}</span>
        {waiting > 0 && (
          <span className="font-semibold text-warning" data-testid="connection-waiting">
            · {t("offline.waiting", { count: waiting })}
          </span>
        )}
        {outbox?.needs_sign_in && (
          <span className="font-semibold text-danger">· {t("offline.signInAgain.short")}</span>
        )}
        {attention > 0 && (
          <span
            className="rounded-full bg-danger px-1.5 text-2xs font-semibold text-white"
            data-testid="connection-attention"
          >
            {attention}
          </span>
        )}
        {hubText && (
          <span
            className="hidden border-s border-border ps-1.5 text-muted lg:inline"
            data-testid="connection-hub"
          >
            {t("offline.hub.label")}: {hubText}
          </span>
        )}
      </button>
      {open && <ConnectionPanel onClose={() => setOpen(false)} />}
    </>
  );
}

function ConnectionPanel({ onClose }: { onClose: () => void }) {
  const { t, formatDate } = useI18n();
  const connection = useConnection();
  const { user } = useSession();
  const [drafts, setDrafts] = useState<Draft[]>([]);
  const [outbox, setOutbox] = useState<OutboxStatus | null>(connection?.outbox ?? null);
  useEffect(() => setOutbox(connection?.outbox ?? null), [connection?.outbox]);
  useEffect(() => {
    if (user) void listDrafts(user.id).then(setDrafts);
  }, [user]);
  if (!connection) return null;
  const when = (iso: string) => formatDate(iso, { dateStyle: "medium", timeStyle: "short" });
  const dismiss = async (kind: "failed" | "overwritten" | "notices") => {
    const s = await outboxDismiss(kind);
    if (s) setOutbox(s);
    connection.refresh();
  };
  const hubText = hubSummary(t, connection.hub);

  return (
    <Dialog open onClose={onClose} title={t("offline.panel.title")} size="lg">
      <div className="flex flex-col gap-4 text-sm" data-testid="connection-panel">
        <p>
          <span className="font-semibold">{t(`offline.link.${connection.link}`)}</span>
          {" — "}
          {connection.kind === "client"
            ? t(`offline.explain.client.${connection.link}`)
            : t(`offline.explain.local.${connection.link}`)}
        </p>
        {hubText && (
          <p>
            <span className="font-semibold">{t("offline.hub.label")}:</span> {hubText}
            {connection.hub?.last_sync_at &&
              ` · ${t("offline.hub.lastSync", { time: when(connection.hub.last_sync_at) })}`}
          </p>
        )}

        {outbox && (
          <section>
            <div className="mb-1 flex items-center justify-between">
              <h3 className="font-semibold text-navy">
                {t("offline.waiting.title", { count: outbox.waiting })}
              </h3>
              {outbox.waiting > 0 && (
                <Button
                  size="sm"
                  variant="secondary"
                  icon={<RefreshCw className="h-3.5 w-3.5" aria-hidden />}
                  onClick={() => void outboxRetry().then(() => connection.refresh())}
                >
                  {t("offline.retry")}
                </Button>
              )}
            </div>
            {outbox.needs_sign_in && (
              <p role="alert" className="mb-1 rounded bg-warning-bg px-2.5 py-2 text-xs">
                {t("offline.signInAgain")}
              </p>
            )}
            {outbox.waiting === 0 ? (
              <p className="text-muted">{t("offline.waiting.none")}</p>
            ) : (
              <ul
                className="divide-y divide-border rounded border border-border"
                data-testid="waiting-list"
              >
                {outbox.items.map((i) => (
                  <li key={i.id} className="flex items-center justify-between gap-2 px-2.5 py-1.5">
                    <span>
                      {describeChange(t, i.method, i.path)}
                      {i.provisional_no && (
                        <span className="ms-1 font-mono text-xs">({i.provisional_no})</span>
                      )}
                    </span>
                    <span className="text-xs text-muted">{when(i.queued_at)}</span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        )}

        {outbox && outbox.failed.length > 0 && (
          <section>
            <div className="mb-1 flex items-center justify-between">
              <h3 className="font-semibold text-danger">{t("offline.failed.title")}</h3>
              <Button size="sm" variant="ghost" onClick={() => void dismiss("failed")}>
                {t("offline.dismiss")}
              </Button>
            </div>
            <p className="mb-1 text-xs text-muted">{t("offline.failed.explain")}</p>
            <ul
              className="divide-y divide-border rounded border border-border"
              data-testid="failed-list"
            >
              {outbox.failed.map((f) => (
                <li key={f.id} className="px-2.5 py-1.5">
                  <div className="font-medium">
                    {describeChange(t, f.method, f.path)}
                    {f.provisional_no && (
                      <span className="ms-1 font-mono text-xs">({f.provisional_no})</span>
                    )}
                    <span className="ms-2 text-xs text-muted">{when(f.queued_at)}</span>
                  </div>
                  <div className="text-xs text-danger">{f.message}</div>
                </li>
              ))}
            </ul>
          </section>
        )}

        {outbox && outbox.overwritten.length > 0 && (
          <section>
            <div className="mb-1 flex items-center justify-between">
              <h3 className="font-semibold text-navy">{t("offline.overwritten.title")}</h3>
              <Button size="sm" variant="ghost" onClick={() => void dismiss("overwritten")}>
                {t("offline.dismiss")}
              </Button>
            </div>
            <p className="mb-1 text-xs text-muted">{t("offline.overwritten.explain")}</p>
            <table className="w-full text-xs" data-testid="overwritten-list">
              <thead>
                <tr className="text-start text-muted">
                  <th className="py-1 text-start font-medium">{t("offline.overwritten.what")}</th>
                  <th className="py-1 text-start font-medium">{t("offline.overwritten.yours")}</th>
                  <th className="py-1 text-start font-medium">{t("offline.overwritten.kept")}</th>
                </tr>
              </thead>
              <tbody>
                {outbox.overwritten.map((o) => (
                  <tr key={o.id} className="border-t border-border">
                    <td className="py-1">
                      {o.label}
                      {o.field !== "status" && (
                        <span className="text-muted">
                          {" · "}
                          {FIELDS[o.field] ? t(FIELDS[o.field]!) : o.field}
                        </span>
                      )}
                    </td>
                    <td className="py-1 line-through">{show(o.yours)}</td>
                    <td className="py-1 font-medium">{show(o.kept)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}

        {outbox && outbox.notices.length > 0 && (
          <section>
            <div className="mb-1 flex items-center justify-between">
              <h3 className="font-semibold text-warning">{t("offline.notices.title")}</h3>
              <Button size="sm" variant="ghost" onClick={() => void dismiss("notices")}>
                {t("offline.dismiss")}
              </Button>
            </div>
            <ul className="list-disc ps-5 text-xs" data-testid="notice-list">
              {outbox.notices.map((n) => (
                <li key={n.id}>
                  {n.code === "overpaid"
                    ? t("offline.notice.overpaid", {
                        receipt: show(n.detail.receipt_no),
                        provisional: show(n.detail.provisional_no),
                        voucher: show(n.detail.voucher_no),
                        amount: show(n.detail.overpaid_by),
                      })
                    : n.code}
                </li>
              ))}
            </ul>
          </section>
        )}

        {outbox && outbox.sent.some((s) => s.provisional_no) && (
          <section>
            <h3 className="mb-1 font-semibold text-navy">{t("offline.receipts.title")}</h3>
            <ul className="text-xs" data-testid="replaced-list">
              {outbox.sent
                .filter((s) => s.provisional_no)
                .map((s) => (
                  <li key={s.item_id}>
                    {t("offline.receipts.replaced", {
                      provisional: s.provisional_no ?? "",
                      real: s.receipt_no ?? "—",
                    })}
                  </li>
                ))}
            </ul>
          </section>
        )}

        <section>
          <h3 className="mb-1 font-semibold text-navy">
            {t("drafts.title", { count: drafts.length })}
          </h3>
          {drafts.length === 0 ? (
            <p className="text-muted">{t("drafts.none")}</p>
          ) : (
            <ul
              className="divide-y divide-border rounded border border-border"
              data-testid="drafts-list"
            >
              {drafts.map((d) => (
                <li key={d.key} className="flex items-center justify-between gap-2 px-2.5 py-1.5">
                  <span>
                    {d.title || d.path}
                    <span className="ms-2 text-xs text-muted">{when(d.at)}</span>
                  </span>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      void deleteDraft(d.key).then(() =>
                        setDrafts((list) => list.filter((x) => x.key !== d.key)),
                      )
                    }
                  >
                    {t("drafts.discard")}
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <p className="mt-1 text-xs text-muted">{t("drafts.explain")}</p>
        </section>
      </div>
    </Dialog>
  );
}
