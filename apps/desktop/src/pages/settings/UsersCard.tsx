import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { LoginRole, Staff, UserAccount } from "@/api/types";
import { useSession } from "@/app/session";
import { useHasFeature } from "@/app/features";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

const ROLES: LoginRole[] = [
  "principal",
  "admin",
  "accountant",
  "teacher",
  "librarian",
  "storekeeper",
];

/** Settings → Users: every login with role, staff link, status and last login. */
export function UsersCard() {
  const { t, formatDate } = useI18n();
  const { user: me } = useSession();
  const toast = useToast();
  const users = useAsync(() => api.users(), []);
  const staff = useAsync(() => api.staff(), []);
  const [creating, setCreating] = useState(false);
  const [resetFor, setResetFor] = useState<UserAccount | null>(null);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const hasFeature = useHasFeature();
  const roleOptions = ROLES.filter(
    (r) =>
      (r !== "librarian" || hasFeature("library")) &&
      (r !== "storekeeper" || hasFeature("inventory")),
  ).map((r) => ({
    value: r,
    label: t(`staff.login.role.${r}`),
  }));
  const withoutLogin = useMemo(
    () => (staff.data ?? []).filter((s: Staff) => !s.user_id && s.status === "active"),
    [staff.data],
  );

  const run = async (work: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await work();
      toast.push({ kind: "success", title: done });
      setCreating(false);
      setResetFor(null);
      setPassword("");
      await users.reload();
      await staff.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title={t("users.title")}>
      <div className="mb-2 flex items-start justify-between gap-3">
        <p className="text-xs text-muted">{t("users.hint")}</p>
        <Button size="sm" onClick={() => setCreating(true)} disabled={withoutLogin.length === 0}>
          {t("staff.login.create")}
        </Button>
      </div>
      {users.data && users.data.length > 0 && (
        <table className="w-full text-sm">
          <thead className="text-xs uppercase text-muted">
            <tr>
              <th className="px-2 py-1.5 text-start">{t("staff.login.username")}</th>
              <th className="px-2 py-1.5 text-start">{t("common.name")}</th>
              <th className="px-2 py-1.5 text-start">{t("staff.login.role")}</th>
              <th className="px-2 py-1.5 text-start">{t("users.staff")}</th>
              <th className="px-2 py-1.5 text-start">{t("users.lastLogin")}</th>
              <th className="px-2 py-1.5 text-start">{t("users.status")}</th>
              <th className="px-2 py-1.5" />
            </tr>
          </thead>
          <tbody>
            {users.data.map((u) => {
              const isSelf = u.id === me?.id;
              return (
                <tr
                  key={u.id}
                  className="border-t border-border"
                  data-testid={`user-${u.username}`}
                >
                  <td className="px-2 py-1.5 font-medium">{u.username}</td>
                  <td className="px-2 py-1.5">{u.full_name}</td>
                  <td className="px-2 py-1.5">
                    <Select
                      aria-label={`${u.username}: ${t("staff.login.role")}`}
                      value={(u.roles[0] as LoginRole) ?? "teacher"}
                      onChange={(e) =>
                        void run(
                          () => api.updateUser(u.id, { role: e.target.value as LoginRole }),
                          t("staff.login.saved"),
                        )
                      }
                      options={roleOptions}
                      disabled={busy}
                    />
                  </td>
                  <td className="px-2 py-1.5 text-muted">{u.staff_name ?? "—"}</td>
                  <td className="px-2 py-1.5 text-muted">
                    {u.last_login_at ? formatDate(u.last_login_at) : t("users.never")}
                  </td>
                  <td className="px-2 py-1.5">
                    <label className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        aria-label={`${u.username}: ${t("staff.login.active")}`}
                        checked={u.is_active}
                        disabled={busy || isSelf}
                        onChange={(e) =>
                          void run(
                            () => api.updateUser(u.id, { is_active: e.target.checked }),
                            t("staff.login.saved"),
                          )
                        }
                      />
                      {u.is_active ? (
                        <Badge tone="success">{t("staff.status.active")}</Badge>
                      ) : (
                        <Badge tone="warning">{t("staff.login.disabled")}</Badge>
                      )}
                    </label>
                  </td>
                  <td className="px-2 py-1.5 text-end">
                    <Button size="sm" variant="ghost" onClick={() => setResetFor(u)}>
                      {t("staff.login.reset")}
                    </Button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      <CreateUserDialog
        open={creating}
        staff={withoutLogin}
        roleOptions={roleOptions}
        busy={busy}
        onClose={() => setCreating(false)}
        onSubmit={(staff_id, username, pw, role) =>
          void run(
            () => api.createUser({ staff_id, username, password: pw, role }),
            t("staff.login.created"),
          )
        }
      />
      <Dialog
        open={resetFor !== null}
        onClose={() => setResetFor(null)}
        title={`${t("staff.login.reset")}: ${resetFor?.username ?? ""}`}
        size="sm"
      >
        <form
          onSubmit={(e: FormEvent) => {
            e.preventDefault();
            if (!resetFor) return;
            void run(
              () => api.resetUserPassword(resetFor.id, password),
              t("staff.login.resetDone"),
            );
          }}
          className="flex flex-col gap-3"
        >
          <Input
            label={t("staff.login.newPassword")}
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="new-password"
            autoFocus
            required
          />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setResetFor(null)}>
              {t("action.cancel")}
            </Button>
            <Button type="submit" loading={busy}>
              {t("staff.login.reset")}
            </Button>
          </div>
        </form>
      </Dialog>
    </Card>
  );
}

function CreateUserDialog({
  open,
  staff,
  roleOptions,
  busy,
  onClose,
  onSubmit,
}: {
  open: boolean;
  staff: Staff[];
  roleOptions: Array<{ value: LoginRole; label: string }>;
  busy: boolean;
  onClose: () => void;
  onSubmit: (staffId: string, username: string, password: string, role: LoginRole) => void;
}) {
  const { t } = useI18n();
  const [staffId, setStaffId] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<LoginRole>("teacher");
  useEffect(() => {
    if (open) setStaffId(staff[0]?.id ?? "");
  }, [open, staff]);
  return (
    <Dialog open={open} onClose={onClose} title={t("staff.login.create")} size="sm">
      <form
        onSubmit={(e: FormEvent) => {
          e.preventDefault();
          onSubmit(staffId, username, password, role);
        }}
        className="flex flex-col gap-3"
      >
        <Select
          label={t("users.staff")}
          value={staffId}
          onChange={(e) => setStaffId(e.target.value)}
          options={staff.map((s) => ({ value: s.id, label: `${s.full_name} (${s.staff_number})` }))}
        />
        <Input
          label={t("staff.login.username")}
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="off"
          required
        />
        <Input
          label={t("staff.login.password")}
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
          required
        />
        <Select
          label={t("staff.login.role")}
          value={role}
          onChange={(e) => setRole(e.target.value as LoginRole)}
          options={roleOptions}
        />
        <p className="text-xs text-muted">{t("staff.login.hint")}</p>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button type="submit" loading={busy}>
            {t("staff.login.create")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
