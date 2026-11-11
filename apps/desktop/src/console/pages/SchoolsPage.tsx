import { useState } from "react";
import type { FormEvent } from "react";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { showValue } from "@/lib/showValue";
import { hub } from "../api";
import type { CreatedInvitation, HubOverwrite, HubParked, HubSchool, Invitation } from "../api";
import { useFail, useHubSession } from "../session";
import { td, th, useWhen } from "../format";
import { PageTitle } from "./common";

const TONE: Record<HubSchool["status"], BadgeTone> = {
  joining: "info",
  active: "success",
  left: "neutral",
  removed: "warning",
  purged: "neutral",
};

function invitationState(i: Invitation): "used" | "revoked" | "expired" | "open" {
  if (i.used_at) return "used";
  if (i.revoked_at) return "revoked";
  if (new Date(i.expires_at).getTime() < Date.now()) return "expired";
  return "open";
}

/**
 * The school register: who has joined, when each last synced, what is waiting; inviting a
 * school; removing one; deleting the archive of a school that has gone.
 */
export function SchoolsPage() {
  const { t } = useI18n();
  const { can } = useHubSession();
  const admin = can("nexus.admin");
  const when = useWhen();
  const fail = useFail();
  const toast = useToast();
  const schools = useAsync(() => hub.schools(), []);
  const invitations = useAsync(() => (admin ? hub.invitations() : Promise.resolve([])), [admin]);
  const [inviting, setInviting] = useState(false);
  const [created, setCreated] = useState<CreatedInvitation | null>(null);
  const [open, setOpen] = useState<HubSchool | null>(null);

  const revoke = async (id: string) => {
    try {
      await hub.revokeInvitation(id);
      await invitations.reload();
    } catch (err) {
      fail(err);
    }
  };

  return (
    <>
      <PageTitle
        title={t("hub.nav.schools")}
        about={t("hub.schools.about")}
        actions={admin && <Button onClick={() => setInviting(true)}>{t("hub.invite")}</Button>}
      />
      <Card padded={false}>
        {schools.loading && !schools.data ? (
          <div className="p-4">
            <Spinner />
          </div>
        ) : !schools.data?.length ? (
          <p className="p-4 text-sm text-muted">{t("hub.schools.none")}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="border-b border-border">
                <tr>
                  <th className={th}>{t("hub.schools.school")}</th>
                  <th className={th}>{t("hub.schools.status")}</th>
                  <th className={th}>{t("hub.schools.lastSync")}</th>
                  <th className={th}>{t("hub.schools.waiting")}</th>
                  <th className={th}>{t("hub.schools.version")}</th>
                  <th className={th} />
                </tr>
              </thead>
              <tbody>
                {schools.data.map((s) => (
                  <tr key={s.id} className="border-t border-border">
                    <td className={td}>
                      <div className="font-medium">{s.name}</div>
                      <div className="text-xs text-muted" dir="ltr">
                        {s.code}
                      </div>
                    </td>
                    <td className={td}>
                      <Badge tone={TONE[s.status]}>{t(`hub.status.${s.status}`)}</Badge>
                    </td>
                    <td className={td}>{when(s.last_sync_at)}</td>
                    <td className={td}>
                      {s.sync
                        ? t("hub.schools.waitingCounts", {
                            pending: s.sync.pending,
                            parked: s.sync.parked,
                            overwrites: s.sync.overwrites,
                          })
                        : "—"}
                    </td>
                    <td className={td} dir="ltr">
                      {s.school_version ?? "—"}
                      {s.edition ? ` · ${s.edition}` : ""}
                    </td>
                    <td className={`${td} text-end`}>
                      {admin && (
                        <Button variant="secondary" size="sm" onClick={() => setOpen(s)}>
                          {t("hub.schools.open")}
                        </Button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {admin && (
        <Card title={t("hub.invitations")} padded={false} className="mt-4">
          {!invitations.data?.length ? (
            <p className="p-4 text-sm text-muted">{t("hub.invitations.none")}</p>
          ) : (
            <table className="w-full">
              <thead className="border-b border-border">
                <tr>
                  <th className={th}>{t("hub.schools.school")}</th>
                  <th className={th}>{t("hub.schools.status")}</th>
                  <th className={th}>{t("hub.invitations.expires")}</th>
                  <th className={th} />
                </tr>
              </thead>
              <tbody>
                {invitations.data.map((i) => {
                  const state = invitationState(i);
                  return (
                    <tr key={i.id} className="border-t border-border">
                      <td className={td}>
                        {i.school_name}
                        {i.note && <div className="text-xs text-muted">{i.note}</div>}
                      </td>
                      <td className={td}>
                        <Badge tone={state === "open" ? "info" : "neutral"}>
                          {t(`hub.invitations.${state}`)}
                        </Badge>
                      </td>
                      <td className={td}>{when(i.expires_at)}</td>
                      <td className={`${td} text-end`}>
                        {state === "open" && (
                          <Button variant="ghost" size="sm" onClick={() => void revoke(i.id)}>
                            {t("hub.invitations.revoke")}
                          </Button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </Card>
      )}

      {inviting && (
        <InviteDialog
          onClose={() => setInviting(false)}
          onCreated={(c) => {
            setInviting(false);
            setCreated(c);
            void invitations.reload();
          }}
        />
      )}
      <Dialog
        open={created !== null}
        onClose={() => setCreated(null)}
        title={t("hub.invite.created", { school: created?.school_name ?? "" })}
        description={t("hub.invite.once")}
        footer={<Button onClick={() => setCreated(null)}>{t("action.close")}</Button>}
      >
        <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-2 text-sm">
          <dt className="text-muted">{t("hub.invite.code")}</dt>
          <dd
            dir="ltr"
            className="select-all break-all text-start font-mono"
            data-testid="join-code"
          >
            {created?.join_code}
          </dd>
          <dt className="text-muted">{t("hub.invite.address")}</dt>
          <dd dir="ltr" className="select-all break-all text-start">
            {created?.hub_url ?? globalThis.location?.origin}
          </dd>
          <dt className="text-muted">{t("hub.invitations.expires")}</dt>
          <dd>{when(created?.expires_at)}</dd>
        </dl>
      </Dialog>
      {open && (
        <SchoolDialog
          school={open}
          onClose={() => setOpen(null)}
          onChanged={(message) => {
            setOpen(null);
            toast.push({ kind: "success", title: message });
            void schools.reload();
          }}
        />
      )}
    </>
  );
}

function InviteDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (c: CreatedInvitation) => void;
}) {
  const { t } = useI18n();
  const fail = useFail();
  const [name, setName] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      onCreated(await hub.invite(name.trim(), note.trim()));
    } catch (err) {
      fail(err);
      setBusy(false);
    }
  };
  return (
    <Dialog open onClose={onClose} title={t("hub.invite")} description={t("hub.invite.about")}>
      <form onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-3">
        <Input
          label={t("hub.invite.school")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoFocus
        />
        <Input
          label={t("hub.invite.note")}
          labelSuffix={t("field.optional")}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button type="submit" loading={busy} disabled={!name.trim()}>
            {t("hub.invite.create")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

/** One school: what sync overwrote, what is waiting, and the things head office may do. */
function SchoolDialog({
  school,
  onClose,
  onChanged,
}: {
  school: HubSchool;
  onClose: () => void;
  onChanged: (message: string) => void;
}) {
  const { t } = useI18n();
  const when = useWhen();
  const fail = useFail();
  const hasData = school.status !== "purged";
  const log = useAsync<HubOverwrite[]>(
    () => (hasData ? hub.overwrites(school.id) : Promise.resolve([])),
    [school.id],
  );
  const parked = useAsync<HubParked[]>(
    () => (hasData ? hub.parked(school.id) : Promise.resolve([])),
    [school.id],
  );
  const [mode, setMode] = useState<"view" | "remove" | "purge">("view");
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);

  const act = async (run: () => Promise<string>) => {
    setBusy(true);
    try {
      onChanged(await run());
    } catch (err) {
      fail(err);
      setBusy(false);
    }
  };
  const remove = () =>
    act(async () => {
      await hub.removeSchool(school.id, text.trim());
      return t("hub.school.removed", { school: school.name });
    });
  const purge = () =>
    act(async () => {
      await hub.purgeSchool(school.id, text.trim());
      return t("hub.school.purged", { school: school.name });
    });
  const resend = () =>
    act(async () => {
      const { queued } = await hub.resend(school.id);
      return t("hub.school.resent", { count: queued });
    });

  return (
    <Dialog open onClose={onClose} title={`${school.name} · ${school.code}`} size="lg">
      <div className="flex flex-col gap-4 text-sm">
        <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1">
          <dt className="text-muted">{t("hub.schools.status")}</dt>
          <dd>{t(`hub.status.${school.status}`)}</dd>
          <dt className="text-muted">{t("hub.school.joined")}</dt>
          <dd>{when(school.joined_at)}</dd>
          <dt className="text-muted">{t("hub.schools.lastSync")}</dt>
          <dd>{when(school.last_sync_at)}</dd>
          {school.left_at && (
            <>
              <dt className="text-muted">{t("hub.school.ended")}</dt>
              <dd>
                {when(school.left_at)}
                {school.leave_reason ? ` · ${school.leave_reason}` : ""}
              </dd>
            </>
          )}
        </dl>

        {parked.data && parked.data.length > 0 && (
          <div className="rounded border border-warning/60 bg-warning-bg p-2 text-xs">
            <div className="font-semibold">
              {t("hub.school.parked", { count: parked.data.length })}
            </div>
            <ul className="mt-1 list-disc ps-5">
              {parked.data.slice(0, 10).map((p) => (
                <li key={`${p.table}:${p.id}`}>
                  <span dir="ltr">{p.table}</span>: {p.reason}
                </li>
              ))}
            </ul>
          </div>
        )}

        <section>
          <h3 className="text-xs font-semibold text-navy">
            {t("nexus.log.title", { count: log.data?.length ?? 0 })}
          </h3>
          <p className="text-xs text-muted">{t("hub.school.logHint")}</p>
          {log.data && log.data.length > 0 && (
            <div className="mt-2 max-h-56 overflow-auto">
              <table className="w-full text-xs">
                <thead className="text-muted">
                  <tr>
                    <th className="py-1 pe-3 text-start font-medium">{t("nexus.log.when")}</th>
                    <th className="py-1 pe-3 text-start font-medium">{t("nexus.log.record")}</th>
                    <th className="py-1 pe-3 text-start font-medium">{t("nexus.log.kept")}</th>
                    <th className="py-1 text-start font-medium">{t("nexus.log.replaced")}</th>
                  </tr>
                </thead>
                <tbody>
                  {log.data.map((o, i) => (
                    <tr key={o.uid ?? i} className="border-t border-border align-top">
                      <td className="whitespace-nowrap py-1 pe-3">{when(o.at)}</td>
                      <td className="py-1 pe-3" dir="ltr">
                        {o.table}.{o.field}
                      </td>
                      <td className="py-1 pe-3">{showValue(o.kept)}</td>
                      <td className="py-1">{showValue(o.discarded)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>

        {mode === "view" && (
          <div className="flex flex-wrap gap-2 border-t border-border pt-3">
            {school.status === "active" && (
              <>
                <Button variant="secondary" loading={busy} onClick={() => void resend()}>
                  {t("hub.school.resend")}
                </Button>
                <Button
                  variant="danger"
                  onClick={() => {
                    setText("");
                    setMode("remove");
                  }}
                >
                  {t("hub.school.remove")}
                </Button>
              </>
            )}
            {(school.status === "left" || school.status === "removed") && (
              <Button
                variant="danger"
                onClick={() => {
                  setText("");
                  setMode("purge");
                }}
              >
                {t("hub.school.purge")}
              </Button>
            )}
          </div>
        )}
        {mode === "remove" && (
          <div className="flex flex-col gap-2 border-t border-border pt-3">
            <p role="alert" className="text-xs text-text">
              {t("hub.school.removeWarning")}
            </p>
            <Input
              label={t("hub.school.reason")}
              value={text}
              onChange={(e) => setText(e.target.value)}
              autoFocus
            />
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setMode("view")}>
                {t("action.cancel")}
              </Button>
              <Button
                variant="danger"
                loading={busy}
                disabled={!text.trim()}
                onClick={() => void remove()}
              >
                {t("hub.school.remove")}
              </Button>
            </div>
          </div>
        )}
        {mode === "purge" && (
          <div className="flex flex-col gap-2 border-t border-border pt-3">
            <p role="alert" className="text-xs text-text">
              {t("hub.school.purgeWarning")}
            </p>
            <Input
              label={t("hub.school.confirmCode", { code: school.code })}
              value={text}
              onChange={(e) => setText(e.target.value)}
              autoFocus
              dir="ltr"
            />
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setMode("view")}>
                {t("action.cancel")}
              </Button>
              <Button
                variant="danger"
                loading={busy}
                disabled={text.trim().toLowerCase() !== school.code.toLowerCase()}
                onClick={() => void purge()}
              >
                {t("hub.school.purge")}
              </Button>
            </div>
          </div>
        )}
      </div>
    </Dialog>
  );
}
