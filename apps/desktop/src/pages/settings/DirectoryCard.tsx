import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { DirectoryGroupRole, DirectorySettings, DirectoryTestResult } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

type Form = Omit<DirectorySettings, "has_bind_password" | "roles">;

/**
 * Settings → Directory login (Standard): the LDAP server, the bind account (password stored
 * sealed, never shown again), where users live, how groups map to roles, and a test.
 */
export function DirectoryCard() {
  const { t } = useI18n();
  const toast = useToast();
  const settings = useAsync(() => api.directorySettings(), []);
  const [form, setForm] = useState<Form | null>(null);
  const [bindPassword, setBindPassword] = useState("");
  const [testUser, setTestUser] = useState("");
  const [result, setResult] = useState<DirectoryTestResult | null>(null);
  const [busy, setBusy] = useState<"save" | "test" | null>(null);
  useEffect(() => {
    if (settings.data) {
      const { has_bind_password: _h, roles: _r, ...rest } = settings.data;
      setForm(rest);
    }
  }, [settings.data]);
  if (!form || !settings.data) {
    return (
      <Card title={t("directory.title")}>
        <Spinner />
      </Card>
    );
  }
  const roles = settings.data.roles;
  const roleOptions = roles.map((r) => ({
    value: r,
    label: t(`staff.login.role.${r}` as TranslationKey),
  }));
  const set = (patch: Partial<Form>) => setForm({ ...form, ...patch });
  const setMapping = (i: number, patch: Partial<DirectoryGroupRole>) =>
    set({ group_roles: form.group_roles.map((m, j) => (j === i ? { ...m, ...patch } : m)) });
  const body = () => ({ ...form, bind_password: bindPassword || null });
  const save = async () => {
    setBusy("save");
    try {
      settings.setData(await api.updateDirectorySettings(body()));
      setBindPassword("");
      toast.push({ kind: "success", title: t("directory.saved") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  const test = async () => {
    setBusy("test");
    setResult(null);
    try {
      setResult(await api.testDirectory({ ...body(), username: testUser || null }));
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  const field = (key: keyof Form, label: string, hint?: string) => (
    <Input
      label={label}
      hint={hint}
      value={String(form[key] ?? "")}
      onChange={(e) => set({ [key]: e.target.value } as Partial<Form>)}
    />
  );
  return (
    <Card
      title={t("directory.title")}
      actions={
        <Badge tone={form.enabled ? "success" : "neutral"}>
          {t(form.enabled ? "directory.on" : "directory.off")}
        </Badge>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("directory.hint")}</p>
      <div className="flex flex-col gap-3">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={form.enabled}
            onChange={(e) => set({ enabled: e.target.checked })}
          />
          {t("directory.enabled")}
        </label>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {field("url", t("directory.url"), t("directory.urlHint"))}
          {field("bind_dn", t("directory.bindDn"))}
          <Input
            label={t("directory.bindPassword")}
            type="password"
            autoComplete="new-password"
            placeholder={
              settings.data.has_bind_password ? t("directory.bindPasswordKept") : undefined
            }
            value={bindPassword}
            onChange={(e) => setBindPassword(e.target.value)}
          />
          {field("user_base_dn", t("directory.baseDn"))}
          {field("user_filter", t("directory.filter"), t("directory.filterHint"))}
          {field("username_attr", t("directory.usernameAttr"))}
          {field("name_attr", t("directory.nameAttr"))}
          {field("email_attr", t("directory.emailAttr"))}
          {field("group_attr", t("directory.groupAttr"))}
          <Select
            label={t("directory.defaultRole")}
            hint={t("directory.defaultRoleHint")}
            value={form.default_role}
            onChange={(e) => set({ default_role: e.target.value })}
            options={[{ value: "", label: t("directory.noDefaultRole") }, ...roleOptions]}
          />
        </div>
        <div>
          <div className="mb-1 flex items-center justify-between">
            <span className="text-xs font-medium uppercase tracking-wide text-muted">
              {t("directory.groups")}
            </span>
            <Button
              size="sm"
              variant="secondary"
              onClick={() =>
                set({
                  group_roles: [...form.group_roles, { group: "", role: roles[0] ?? "teacher" }],
                })
              }
              data-testid="add-mapping"
            >
              {t("directory.addGroup")}
            </Button>
          </div>
          <p className="mb-2 text-xs text-muted">{t("directory.groupsHint")}</p>
          {form.group_roles.length === 0 ? (
            <p className="text-sm text-muted">{t("directory.noGroups")}</p>
          ) : (
            <div className="flex flex-col gap-2">
              {form.group_roles.map((m, i) => (
                <div key={i} className="flex flex-wrap items-end gap-2">
                  <Input
                    aria-label={`${t("directory.group")} ${i + 1}`}
                    placeholder="CN=Teachers,OU=Groups,DC=school,DC=local"
                    value={m.group}
                    onChange={(e) => setMapping(i, { group: e.target.value })}
                    className="min-w-[20rem] flex-1"
                  />
                  <Select
                    aria-label={`${t("staff.login.role")} ${i + 1}`}
                    value={m.role}
                    onChange={(e) => setMapping(i, { role: e.target.value })}
                    options={roleOptions}
                    className="w-40"
                  />
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => set({ group_roles: form.group_roles.filter((_, j) => j !== i) })}
                  >
                    ×
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
        {field("login_hint", t("directory.loginHint"), t("directory.loginHintHint"))}
        <div className="flex flex-wrap items-end gap-2 rounded border border-border p-2">
          <Input
            label={t("directory.testUser")}
            value={testUser}
            onChange={(e) => setTestUser(e.target.value)}
            className="w-48"
          />
          <Button
            variant="secondary"
            loading={busy === "test"}
            onClick={() => void test()}
            data-testid="test-directory"
          >
            {t("directory.test")}
          </Button>
          {result && (
            <p
              role="status"
              className={`text-sm ${result.bind_ok ? "text-success" : "text-danger"}`}
              data-testid="test-result"
            >
              {result.message}
            </p>
          )}
        </div>
        <div className="flex justify-end">
          <Button
            loading={busy === "save"}
            onClick={() => void save()}
            data-testid="save-directory"
          >
            {t("action.save")}
          </Button>
        </div>
      </div>
    </Card>
  );
}
