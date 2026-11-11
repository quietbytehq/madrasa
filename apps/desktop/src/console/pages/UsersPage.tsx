import { useState } from "react";
import type { FormEvent } from "react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { HUB_PERMISSIONS, hub } from "../api";
import type { HubPermission, HubUser } from "../api";
import { useFail, useHubSession } from "../session";
import { td, th, useWhen } from "../format";
import { PageTitle } from "./common";

const PERMISSION_LABEL = {
  "nexus.admin": "hub.permission.admin",
  "nexus.reports": "hub.permission.reports",
  "nexus.escrow": "hub.permission.escrow",
} as const;

/** Head office accounts: who may sign in to this console and what each may do. */
export function UsersPage() {
  const { t } = useI18n();
  const { me } = useHubSession();
  const when = useWhen();
  const toast = useToast();
  const users = useAsync(() => hub.users(), []);
  const [editing, setEditing] = useState<HubUser | "new" | null>(null);

  return (
    <>
      <PageTitle
        title={t("hub.nav.users")}
        about={t("hub.users.about")}
        actions={<Button onClick={() => setEditing("new")}>{t("hub.users.add")}</Button>}
      />
      <Card padded={false}>
        {users.loading && !users.data ? (
          <div className="p-4">
            <Spinner />
          </div>
        ) : (
          <table className="w-full">
            <thead className="border-b border-border">
              <tr>
                <th className={th}>{t("hub.users.fullName")}</th>
                <th className={th}>{t("hub.users.may")}</th>
                <th className={th}>{t("hub.users.lastLogin")}</th>
                <th className={th} />
              </tr>
            </thead>
            <tbody>
              {(users.data ?? []).map((u) => (
                <tr key={u.id} className="border-t border-border">
                  <td className={td}>
                    <div className="font-medium">
                      {u.full_name}
                      {u.id === me.user.id && (
                        <span className="ms-1 text-xs font-normal text-muted">
                          ({t("hub.users.you")})
                        </span>
                      )}
                    </div>
                    <div className="text-xs text-muted" dir="ltr">
                      {u.username}
                    </div>
                  </td>
                  <td className={td}>
                    <div className="flex flex-wrap gap-1">
                      {!u.is_active && <Badge tone="danger">{t("hub.users.disabled")}</Badge>}
                      {u.permissions.length === 0 && u.is_active && "—"}
                      {u.permissions.map((p) => (
                        <Badge key={p} tone="info">
                          {t(PERMISSION_LABEL[p as HubPermission] ?? "hub.permission.admin")}
                        </Badge>
                      ))}
                    </div>
                  </td>
                  <td className={td}>{when(u.last_login_at)}</td>
                  <td className={`${td} text-end`}>
                    <Button variant="secondary" size="sm" onClick={() => setEditing(u)}>
                      {t("action.edit")}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
      {editing && (
        <UserDialog
          user={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            toast.push({ kind: "success", title: t("hub.users.saved") });
            void users.reload();
          }}
        />
      )}
    </>
  );
}

function UserDialog({
  user,
  onClose,
  onSaved,
}: {
  user: HubUser | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useI18n();
  const fail = useFail();
  const [fullName, setFullName] = useState(user?.full_name ?? "");
  const [username, setUsername] = useState(user?.username ?? "");
  const [email, setEmail] = useState(user?.email ?? "");
  const [password, setPassword] = useState("");
  const [permissions, setPermissions] = useState<string[]>(user?.permissions ?? ["nexus.reports"]);
  const [active, setActive] = useState(user?.is_active ?? true);
  const [busy, setBusy] = useState(false);

  const toggle = (p: string) =>
    setPermissions((list) => (list.includes(p) ? list.filter((x) => x !== p) : [...list, p]));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      if (user) {
        await hub.updateUser(user.id, {
          full_name: fullName.trim(),
          email: email.trim() || null,
          permissions,
          is_active: active,
        });
        if (password) await hub.setPassword(user.id, password);
      } else {
        await hub.createUser({
          username: username.trim(),
          full_name: fullName.trim(),
          email: email.trim() || null,
          password,
          permissions,
        });
      }
      onSaved();
    } catch (err) {
      fail(err);
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={onClose} title={user ? user.full_name : t("hub.users.add")}>
      <form onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-3">
        <div className="grid gap-3 sm:grid-cols-2">
          <Input
            label={t("hub.users.fullName")}
            value={fullName}
            onChange={(e) => setFullName(e.target.value)}
            autoFocus
          />
          <Input
            label={t("hub.login.username")}
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            disabled={user !== null}
            dir="ltr"
          />
          <Input
            label={t("hub.users.email")}
            labelSuffix={t("field.optional")}
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            dir="ltr"
          />
          <Input
            label={user ? t("hub.users.newPassword") : t("hub.login.password")}
            labelSuffix={user ? t("field.optional") : undefined}
            hint={t("hub.users.passwordHint")}
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="new-password"
          />
        </div>
        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1 text-xs font-medium text-text">{t("hub.users.may")}</legend>
          {HUB_PERMISSIONS.map((p) => (
            <label key={p} className="flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={permissions.includes(p)}
                onChange={() => toggle(p)}
              />
              <span>
                {t(PERMISSION_LABEL[p])}
                <span className="block text-xs text-muted">
                  {t(`${PERMISSION_LABEL[p]}.about`)}
                </span>
              </span>
            </label>
          ))}
        </fieldset>
        {user && (
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={active} onChange={(e) => setActive(e.target.checked)} />
            {t("hub.users.active")}
          </label>
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            type="submit"
            loading={busy}
            disabled={!fullName.trim() || (!user && (!username.trim() || !password))}
          >
            {t("action.save")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
