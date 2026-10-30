import { useState } from "react";
import type { FormEvent } from "react";
import { DatabaseBackup } from "lucide-react";
import { api } from "@/api/endpoints";
import type { BackupFile, BackupSettings, RestorePreview } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function BackupPage() {
  const { t, formatDate } = useI18n();
  const { can, logout } = useSession();
  const toast = useToast();
  const manage = can("backup.manage");
  const status = useAsync(() => api.backupStatus(), []);
  const files = useAsync(() => (manage ? api.backupFiles() : Promise.resolve([])), [manage]);
  const runs = useAsync(() => (manage ? api.backupRuns(10) : Promise.resolve([])), [manage]);
  const [running, setRunning] = useState(false);
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [otherPath, setOtherPath] = useState("");
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const refresh = () => {
    void status.reload();
    void files.reload();
    void runs.reload();
  };
  const runNow = async () => {
    setRunning(true);
    try {
      const run = await api.runBackup();
      toast.push({ kind: "success", title: t("backup.run.done", { file: run.file_path ?? "" }) });
      refresh();
    } catch (err) {
      fail(err);
      refresh();
    } finally {
      setRunning(false);
    }
  };
  const check = async (path: string) => {
    try {
      setPreview(await api.previewRestore(path));
    } catch (err) {
      fail(err);
    }
  };
  const s = status.data;
  return (
    <>
      <PageHeader
        title={t("backup.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("backup.title") }]}
        actions={
          manage && (
            <Button
              loading={running}
              icon={<DatabaseBackup className="h-4 w-4" aria-hidden />}
              onClick={() => void runNow()}
            >
              {t("backup.runNow")}
            </Button>
          )
        }
      />
      {!s ? (
        <div className="flex justify-center py-10">
          <Spinner size="lg" />
        </div>
      ) : (
        <div className="grid gap-4 xl:grid-cols-2">
          <Card title={t("backup.status.last")}>
            <div
              className={`text-2xl font-semibold tabular-nums ${s.stale ? "text-danger" : "text-navy"}`}
            >
              {s.last_backup_at ? formatDate(s.last_backup_at) : t("backup.status.never")}
            </div>
            <p role="status" className={`mt-1 text-sm ${s.stale ? "text-danger" : "text-success"}`}>
              {s.stale ? t("backup.status.stale") : t("backup.status.ok")}
            </p>
            <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
              <dt className="text-muted">{t("backup.settings")}</dt>
              <dd>
                {s.enabled
                  ? t("backup.status.nextNightly", { time: s.time })
                  : t("backup.status.disabled")}
              </dd>
              <dt className="text-muted">{t("backup.status.folder")}</dt>
              <dd className="break-all font-mono">{s.directory}</dd>
            </dl>
            {s.last_error && (
              <p className="mt-2 text-xs text-danger">
                {t("backup.status.lastError", { error: s.last_error })}
              </p>
            )}
            {!s.unattended_ready && manage && <UnlockForm onDone={refresh} onError={fail} />}
          </Card>
          {manage && (
            <SettingsCard
              onSaved={() => {
                toast.push({ kind: "success", title: t("common.saved") });
                refresh();
              }}
              onError={fail}
            />
          )}
          {manage && (
            <Card title={t("backup.files")} padded={false} className="xl:col-span-2">
              {files.data && files.data.length === 0 ? (
                <EmptyState title={t("backup.files.empty")} />
              ) : (
                <table className="w-full text-sm">
                  <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                    <tr>
                      <th className="px-3 py-2 text-start">{t("backup.files.name")}</th>
                      <th className="px-3 py-2 text-end">{t("backup.files.size")}</th>
                      <th className="px-3 py-2 text-start">{t("backup.files.modified")}</th>
                      <th className="px-3 py-2 text-end">{t("common.actions")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(files.data ?? []).map((f: BackupFile) => (
                      <tr key={f.path} className="border-t border-border">
                        <td className="px-3 py-1.5 font-mono text-xs">{f.name}</td>
                        <td className="px-3 py-1.5 text-end tabular-nums">{formatSize(f.size)}</td>
                        <td className="px-3 py-1.5">{formatDate(f.modified)}</td>
                        <td className="px-3 py-1.5 text-end">
                          <Button size="sm" variant="secondary" onClick={() => void check(f.path)}>
                            {t("backup.restore")}
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
              <form
                className="flex flex-wrap items-end gap-2 border-t border-border p-3"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (otherPath.trim()) void check(otherPath.trim());
                }}
              >
                <Input
                  label={t("backup.files.otherPath")}
                  placeholder={t("backup.files.path")}
                  value={otherPath}
                  onChange={(e) => setOtherPath(e.target.value)}
                  className="w-96"
                />
                <Button type="submit" variant="secondary" disabled={!otherPath.trim()}>
                  {t("backup.restore.preview")}
                </Button>
              </form>
            </Card>
          )}
          {manage && <PasswordCard onError={fail} />}
          {manage && (
            <Card title={t("backup.runs")} padded={false}>
              {runs.data && runs.data.length === 0 ? (
                <EmptyState title={t("backup.runs.empty")} />
              ) : (
                <ul className="divide-y divide-border text-sm">
                  {(runs.data ?? []).map((r) => (
                    <li key={r.id} className="flex flex-wrap items-center gap-3 px-4 py-2">
                      <span className="text-muted">{formatDate(r.started_at)}</span>
                      <span>{t(`backup.runs.${r.trigger}` as TranslationKey)}</span>
                      <Badge
                        tone={
                          r.status === "ok" ? "success" : r.status === "failed" ? "danger" : "info"
                        }
                      >
                        {t(`backup.runs.${r.status}` as TranslationKey)}
                      </Badge>
                      <span className="flex-1" />
                      <span className="font-mono text-xs text-muted">
                        {r.file_path?.split(/[\\/]/).pop() ?? r.error ?? ""}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          )}
        </div>
      )}
      {preview && (
        <RestoreDialog
          preview={preview}
          onClose={() => setPreview(null)}
          onRestored={(rows, date) => {
            setPreview(null);
            toast.push({
              kind: "success",
              title: t("backup.restore.done", { rows, date: formatDate(date) }),
            });
            void logout();
          }}
          onError={fail}
        />
      )}
    </>
  );
}

function UnlockForm({ onDone, onError }: { onDone: () => void; onError: (e: unknown) => void }) {
  const { t } = useI18n();
  const toast = useToast();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api.unlockBackups({ password });
      toast.push({ kind: "success", title: t("backup.unlock.done") });
      setPassword("");
      onDone();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <form
      onSubmit={submit}
      className="mt-3 flex flex-wrap items-end gap-2 rounded border border-warning bg-warning-bg p-3"
    >
      <p className="w-full text-xs text-text">{t("backup.status.unlockNeeded")}</p>
      <Input
        label={t("backup.restore.password")}
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        className="w-64"
      />
      <Button type="submit" loading={busy} disabled={!password}>
        {t("backup.unlock")}
      </Button>
    </form>
  );
}

function SettingsCard({
  onSaved,
  onError,
}: {
  onSaved: () => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const settings = useAsync(() => api.backupSettings(), []);
  const [edit, setEdit] = useState<BackupSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const s = edit ?? settings.data;
  if (!s)
    return (
      <Card title={t("backup.settings")}>
        <Spinner />
      </Card>
    );
  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      settings.setData(await api.saveBackupSettings({ ...s, keep: Number(s.keep) || 30 }));
      setEdit(null);
      onSaved();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title={t("backup.settings")}>
      <form onSubmit={save} noValidate className="flex flex-col gap-3">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={s.enabled}
            onChange={(e) => setEdit({ ...s, enabled: e.target.checked })}
          />
          {t("backup.settings.enabled")}
        </label>
        <div className="flex flex-wrap gap-3">
          <Input
            label={t("backup.settings.time")}
            type="time"
            value={s.time}
            onChange={(e) => setEdit({ ...s, time: e.target.value })}
            className="w-32"
          />
          <Input
            label={t("backup.settings.keep")}
            type="number"
            min={1}
            max={3650}
            hint={t("backup.settings.keepHint")}
            value={s.keep}
            onChange={(e) => setEdit({ ...s, keep: Number(e.target.value) })}
            className="w-32"
          />
        </div>
        <Input
          label={t("backup.settings.directory")}
          labelSuffix={t("field.optional")}
          hint={t("backup.settings.directoryHint")}
          value={s.directory ?? ""}
          onChange={(e) => setEdit({ ...s, directory: e.target.value || null })}
        />
        <div className="flex justify-end">
          <Button type="submit" loading={busy} disabled={!edit}>
            {t("action.save")}
          </Button>
        </div>
      </form>
    </Card>
  );
}

function PasswordCard({ onError }: { onError: (e: unknown) => void }) {
  const { t } = useI18n();
  const toast = useToast();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (next !== confirm) return setError(t("backup.password.mismatch"));
    setError(null);
    setBusy(true);
    try {
      await api.changeBackupPassword({ current_password: current, new_password: next });
      toast.push({ kind: "success", title: t("backup.password.changed") });
      setCurrent("");
      setNext("");
      setConfirm("");
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title={t("backup.password")}>
      <p
        role="note"
        className="mb-3 rounded border border-warning bg-warning-bg px-3 py-2 text-xs text-text"
      >
        {t("backup.password.hint")}
      </p>
      <form onSubmit={submit} noValidate className="flex flex-col gap-3">
        <Input
          label={t("backup.password.current")}
          type="password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
        />
        <Input
          label={t("backup.password.new")}
          type="password"
          value={next}
          onChange={(e) => setNext(e.target.value)}
        />
        <Input
          label={t("backup.password.confirm")}
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          error={error}
        />
        <div className="flex justify-end">
          <Button type="submit" loading={busy} disabled={!current || !next || !confirm}>
            {t("backup.password.change")}
          </Button>
        </div>
      </form>
    </Card>
  );
}

const COUNT_KEYS = [
  "students",
  "staff",
  "guardians",
  "users",
  "fee_vouchers",
  "fee_receipts",
  "attendance_sheets",
  "mark_sheets",
  "audit_log",
] as const;

function RestoreDialog({
  preview,
  onClose,
  onRestored,
  onError,
}: {
  preview: RestorePreview;
  onClose: () => void;
  onRestored: (rows: number, date: string) => void;
  onError: (e: unknown) => void;
}) {
  const { t, formatDate } = useI18n();
  const [useRecovery, setUseRecovery] = useState(false);
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const r = await api.restoreBackup({
        path: preview.path,
        password: useRecovery ? undefined : secret,
        recovery_key: useRecovery ? secret : undefined,
      });
      onRestored(r.rows, r.meta.created_at);
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open onClose={onClose} title={t("backup.restore.previewTitle")} size="md">
      <p
        role="alert"
        className="mb-3 rounded border border-danger bg-danger-bg px-3 py-2 text-sm text-text"
      >
        {t("backup.restore.warning")}
      </p>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        <dt className="text-muted">{t("backup.restore.backupDate")}</dt>
        <dd>{formatDate(preview.meta.created_at)}</dd>
        <dt className="text-muted">{t("backup.restore.school")}</dt>
        <dd>
          {preview.meta.school_name} · {preview.meta.school_code}
        </dd>
        <dt className="text-muted">{t("backup.restore.appVersion")}</dt>
        <dd>{preview.meta.app_version}</dd>
        <dt className="text-muted">{t("backup.files.size")}</dt>
        <dd>{formatSize(preview.size)}</dd>
      </dl>
      <table className="mt-3 w-full text-xs">
        <thead className="bg-hover text-muted">
          <tr>
            <th className="px-2 py-1 text-start">{t("backup.restore.counts")}</th>
            <th className="px-2 py-1 text-end">{t("backup.restore.inBackup")}</th>
            <th className="px-2 py-1 text-end">{t("backup.restore.now")}</th>
          </tr>
        </thead>
        <tbody>
          {COUNT_KEYS.map((k) => (
            <tr key={k} className="border-t border-border">
              <td className="px-2 py-1">{t(`backup.table.${k}`)}</td>
              <td className="px-2 py-1 text-end tabular-nums">{preview.meta.counts[k] ?? 0}</td>
              <td className="px-2 py-1 text-end tabular-nums">{preview.current_counts[k] ?? 0}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {!preview.schema_matches ? (
        <p role="alert" className="mt-3 text-sm text-danger">
          {t("backup.restore.schemaMismatch")}
        </p>
      ) : (
        <form onSubmit={submit} noValidate className="mt-3 flex flex-col gap-2">
          <Input
            label={useRecovery ? t("backup.restore.recoveryKey") : t("backup.restore.password")}
            type={useRecovery ? "text" : "password"}
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            autoFocus
          />
          <button
            type="button"
            className="self-start text-xs text-blue hover:underline"
            onClick={() => {
              setUseRecovery((v) => !v);
              setSecret("");
            }}
          >
            {useRecovery ? t("backup.restore.usePassword") : t("backup.restore.useRecovery")}
          </button>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={onClose}>
              {t("action.cancel")}
            </Button>
            <Button type="submit" variant="danger" loading={busy} disabled={!secret}>
              {t("backup.restore.confirm")}
            </Button>
          </div>
        </form>
      )}
    </Dialog>
  );
}
