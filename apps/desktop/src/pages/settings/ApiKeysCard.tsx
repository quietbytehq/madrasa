import { useState } from "react";
import { api } from "@/api/endpoints";
import type { ApiKey, ApiKeyCall, ApiKeysView, CreatedApiKey } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

/**
 * Settings → API: keys for the read-only API (name, scopes, allowed addresses, expiry), the
 * key shown once at creation, revocation and the call log, plus the endpoint reference.
 */
export function ApiKeysCard() {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const view = useAsync<ApiKeysView>(() => api.apiKeys(), []);
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<CreatedApiKey | null>(null);
  const [calls, setCalls] = useState<{ key: ApiKey; rows: ApiKeyCall[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const revoke = async (k: ApiKey) => {
    if (!window.confirm(t("apikeys.revokeConfirm", { name: k.name }))) return;
    setBusy(true);
    try {
      await api.revokeApiKey(k.id);
      toast.push({ kind: "success", title: t("apikeys.revoked") });
      await view.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const showCalls = async (k: ApiKey) => {
    try {
      setCalls({ key: k, rows: await api.apiKeyCalls(k.id) });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    }
  };
  const data = view.data;
  return (
    <Card
      title={t("apikeys.title")}
      actions={
        <Button size="sm" onClick={() => setCreating(true)} data-testid="add-key">
          {t("apikeys.add")}
        </Button>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("apikeys.hint")}</p>
      {!data ? (
        <Spinner />
      ) : data.keys.length === 0 ? (
        <EmptyState title={t("apikeys.empty")} />
      ) : (
        <Table<ApiKey>
          rows={data.keys}
          rowKey={(k) => k.id}
          pageSize={0}
          columns={[
            { key: "name", header: t("apikeys.name") },
            {
              key: "key_prefix",
              header: t("apikeys.key"),
              render: (k) => <code className="text-xs">{k.key_prefix}…</code>,
            },
            {
              key: "scopes",
              header: t("apikeys.scopes"),
              render: (k) => (
                <span className="flex flex-wrap gap-1">
                  {k.scopes.map((s) => (
                    <Badge key={s} tone="neutral">
                      {t(`apikeys.scope.${s}` as TranslationKey)}
                    </Badge>
                  ))}
                </span>
              ),
            },
            {
              key: "ip_allow",
              header: t("apikeys.ipAllow"),
              render: (k) => (k.ip_allow.length ? k.ip_allow.join(", ") : t("apikeys.anyAddress")),
            },
            {
              key: "expires_at",
              header: t("apikeys.expires"),
              render: (k) => (k.expires_at ? formatDate(k.expires_at) : "—"),
            },
            {
              key: "last_used_at",
              header: t("apikeys.lastUsed"),
              render: (k) => (k.last_used_at ? formatDate(k.last_used_at) : t("users.never")),
            },
            {
              key: "status",
              header: t("apikeys.status"),
              render: (k) =>
                k.revoked_at ? (
                  <Badge tone="danger">{t("apikeys.revokedState")}</Badge>
                ) : k.expires_at && new Date(k.expires_at) < new Date() ? (
                  <Badge tone="warning">{t("apikeys.expired")}</Badge>
                ) : (
                  <Badge tone="success">{t("apikeys.active")}</Badge>
                ),
            },
            {
              key: "actions",
              header: "",
              render: (k) => (
                <div className="flex justify-end gap-1">
                  <Button size="sm" variant="ghost" onClick={() => void showCalls(k)}>
                    {t("apikeys.calls")}
                  </Button>
                  {!k.revoked_at && (
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => void revoke(k)}
                    >
                      {t("apikeys.revoke")}
                    </Button>
                  )}
                </div>
              ),
            },
          ]}
        />
      )}
      {data && <Reference view={data} />}
      {creating && data && (
        <CreateDialog
          scopes={data.scopes}
          onClose={() => setCreating(false)}
          onCreated={(k) => {
            setCreated(k);
            void view.reload();
          }}
        />
      )}
      {created && (
        <Dialog
          open
          onClose={() => setCreated(null)}
          title={t("apikeys.createdTitle")}
          footer={<Button onClick={() => setCreated(null)}>{t("action.close")}</Button>}
        >
          <p className="mb-2 text-sm">{t("apikeys.createdHint")}</p>
          <code
            className="block break-all rounded border border-border bg-bg p-2 text-sm"
            data-testid="new-key"
          >
            {created.key}
          </code>
        </Dialog>
      )}
      {calls && (
        <Dialog
          open
          onClose={() => setCalls(null)}
          size="lg"
          title={`${t("apikeys.calls")}: ${calls.key.name}`}
        >
          {calls.rows.length === 0 ? (
            <p className="text-sm text-muted">{t("apikeys.noCalls")}</p>
          ) : (
            <table className="w-full text-sm">
              <tbody>
                {calls.rows.map((c) => (
                  <tr key={c.id} className="border-b border-border/50">
                    <td className="py-1 pe-2 text-muted">
                      {new Date(c.called_at).toLocaleString()}
                    </td>
                    <td className="py-1 pe-2">{c.method}</td>
                    <td className="py-1 pe-2 font-mono text-xs">{c.path}</td>
                    <td className="py-1 pe-2">{c.status}</td>
                    <td className="py-1 text-muted">{c.ip}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Dialog>
      )}
    </Card>
  );
}

function CreateDialog({
  scopes,
  onClose,
  onCreated,
}: {
  scopes: string[];
  onClose: () => void;
  onCreated: (k: CreatedApiKey) => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [name, setName] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [ips, setIps] = useState("");
  const [expires, setExpires] = useState("");
  const [busy, setBusy] = useState(false);
  const toggle = (s: string) =>
    setPicked(picked.includes(s) ? picked.filter((x) => x !== s) : [...picked, s]);
  const save = async () => {
    setBusy(true);
    try {
      const k = await api.createApiKey({
        name: name.trim(),
        scopes: picked,
        ip_allow: ips
          .split(/[\n,;]+/)
          .map((s) => s.trim())
          .filter(Boolean),
        expires_at: expires ? new Date(`${expires}T23:59:59`).toISOString() : null,
      });
      onCreated(k);
      onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("apikeys.add")}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!name.trim() || picked.length === 0}
            onClick={() => void save()}
            data-testid="save-key"
          >
            {t("action.save")}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <Input
          label={t("apikeys.name")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoFocus
        />
        <fieldset>
          <legend className="mb-1 text-xs font-medium text-text">{t("apikeys.scopes")}</legend>
          <div className="flex flex-wrap gap-3">
            {scopes.map((s) => (
              <label key={s} className="flex items-center gap-1 text-sm">
                <input type="checkbox" checked={picked.includes(s)} onChange={() => toggle(s)} />
                {t(`apikeys.scope.${s}` as TranslationKey)}
              </label>
            ))}
          </div>
        </fieldset>
        <Input
          label={t("apikeys.ipAllow")}
          hint={t("apikeys.ipAllowHint")}
          value={ips}
          onChange={(e) => setIps(e.target.value)}
        />
        <Input
          label={t("apikeys.expires")}
          type="date"
          value={expires}
          onChange={(e) => setExpires(e.target.value)}
          className="w-44"
        />
      </div>
    </Dialog>
  );
}

const ENDPOINTS: { path: string; scope: string; key: TranslationKey; params: string }[] = [
  { path: "/", scope: "any", key: "apikeys.doc.index", params: "" },
  {
    path: "/students",
    scope: "students",
    key: "apikeys.doc.students",
    params: "academic_year_id, section_id, status, updated_since, page, page_size",
  },
  { path: "/sections", scope: "students", key: "apikeys.doc.sections", params: "academic_year_id" },
  {
    path: "/attendance/daily",
    scope: "attendance",
    key: "apikeys.doc.attendance",
    params: "academic_year_id, date, section_id",
  },
  {
    path: "/fees/balances",
    scope: "fees",
    key: "apikeys.doc.fees",
    params: "academic_year_id, section_id",
  },
  {
    path: "/exams/assessments",
    scope: "exams",
    key: "apikeys.doc.assessments",
    params: "academic_year_id",
  },
  {
    path: "/exams/results",
    scope: "exams",
    key: "apikeys.doc.results",
    params: "assessment_id, section_id",
  },
  {
    path: "/timetable",
    scope: "timetable",
    key: "apikeys.doc.timetable",
    params: "academic_year_id, section_id",
  },
  { path: "/library/loans", scope: "library", key: "apikeys.doc.loans", params: "state" },
  { path: "/openapi.json", scope: "any", key: "apikeys.doc.openapi", params: "" },
];

/** The endpoint reference with curl examples. */
function Reference({ view }: { view: ApiKeysView }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const base = `https://<server>:<port>${view.base_path}`;
  return (
    <div className="mt-4">
      <button
        type="button"
        className="text-sm text-blue hover:underline"
        onClick={() => setOpen(!open)}
        data-testid="toggle-docs"
      >
        {open ? t("apikeys.docs.hide") : t("apikeys.docs.show")}
      </button>
      {open && (
        <div className="mt-2 flex flex-col gap-3 text-sm" data-testid="docs">
          <p className="text-xs text-muted">
            {t("apikeys.docs.intro", { n: view.rate_limit_per_minute })}
          </p>
          <pre className="overflow-auto rounded border border-border bg-bg p-2 text-xs">{`curl -H "Authorization: Bearer mk_…" "${base}/students?page_size=100"\ncurl -H "X-Api-Key: mk_…" -H 'If-None-Match: W/"…"' "${base}/attendance/daily?date=2026-04-06"`}</pre>
          <table className="w-full text-xs">
            <thead className="text-muted">
              <tr>
                <th className="py-1 text-start">{t("apikeys.docs.path")}</th>
                <th className="py-1 text-start">{t("apikeys.docs.scope")}</th>
                <th className="py-1 text-start">{t("apikeys.docs.what")}</th>
                <th className="py-1 text-start">{t("apikeys.docs.params")}</th>
              </tr>
            </thead>
            <tbody>
              {ENDPOINTS.map((e) => (
                <tr key={e.path} className="border-t border-border/50">
                  <td className="py-1 pe-2 font-mono">{e.path}</td>
                  <td className="py-1 pe-2">{e.scope}</td>
                  <td className="py-1 pe-2">{t(e.key)}</td>
                  <td className="py-1 text-muted">{e.params}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
