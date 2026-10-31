import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Plus } from "lucide-react";
import { api } from "@/api/endpoints";
import type { Assignment, Gender, LoginRole, Section, Staff, Subject } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import { ExportMenu } from "@/components/ExportMenu";
import { GENDERS, genderLabel } from "../students/common";

export function StaffPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("teachers.manage");
  const toast = useToast();
  const [q, setQ] = useState("");
  const list = useAsync(() => api.staff(q || undefined), [q]);
  const sections = useAsync(() => api.sections(), []);
  const subjects = useAsync(() => api.subjects(), []);
  const [editing, setEditing] = useState<Staff | "new" | null>(null);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });

  return (
    <>
      <PageHeader
        title={t("staff.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("staff.title") }]}
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <ExportMenu path="staff" params={{ q: q || undefined }} />
            {manage && (
              <Button
                icon={<Plus className="h-4 w-4" aria-hidden />}
                onClick={() => setEditing("new")}
              >
                {t("staff.add")}
              </Button>
            )}
          </div>
        }
      />
      <div className="mb-3">
        <Input
          type="search"
          aria-label={t("staff.search")}
          placeholder={t("staff.search")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          className="w-80"
        />
      </div>
      {list.loading && !list.data ? (
        <div className="flex justify-center py-10">
          <Spinner size="lg" />
        </div>
      ) : (list.data?.length ?? 0) === 0 ? (
        <Card>
          <EmptyState title={t("staff.empty")} />
        </Card>
      ) : (
        <Table
          rows={list.data ?? []}
          rowKey={(s) => s.id}
          onRowClick={(s) => setEditing(s)}
          pageSize={50}
          initialSort={{ key: "full_name", direction: "asc" }}
          columns={[
            { key: "staff_number", header: t("staff.number"), sortable: true },
            { key: "full_name", header: t("common.name"), sortable: true },
            { key: "designation", header: t("staff.designation"), sortable: true },
            { key: "phone", header: t("common.phone") },
            {
              key: "username",
              header: t("staff.login"),
              render: (s) =>
                s.username ?? <span className="text-muted">{t("staff.login.none")}</span>,
            },
            {
              key: "roles",
              header: t("staff.roles"),
              accessor: (s) => s.roles.join(", "),
              render: (s) =>
                s.roles.map((r) => (
                  <Badge key={r} tone="info" className="me-1">
                    {r}
                  </Badge>
                )),
            },
            {
              key: "status",
              header: t("common.status"),
              render: (s) => (
                <Badge tone={s.status === "active" ? "success" : "neutral"}>
                  {t(`staff.status.${s.status}`)}
                </Badge>
              ),
            },
          ]}
        />
      )}
      {editing && (
        <StaffDialog
          staff={editing === "new" ? null : editing}
          sections={sections.data ?? []}
          subjects={subjects.data ?? []}
          manage={manage}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            void list.reload();
          }}
          onError={fail}
        />
      )}
    </>
  );
}

function StaffDialog({
  staff,
  sections,
  subjects,
  manage,
  onClose,
  onSaved,
  onError,
}: {
  staff: Staff | null;
  sections: Section[];
  subjects: Subject[];
  manage: boolean;
  onClose: () => void;
  onSaved: () => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [form, setForm] = useState({
    staff_number: staff?.staff_number ?? "",
    full_name: staff?.full_name ?? "",
    father_name: staff?.father_name ?? "",
    gender: (staff?.gender ?? "other") as Gender,
    phone: staff?.phone ?? "",
    email: staff?.email ?? "",
    cnic: staff?.cnic ?? "",
    designation: staff?.designation ?? "",
    joined_on: staff?.joined_on ?? "",
    status: staff?.status ?? "active",
  });
  const [assignments, setAssignments] = useState<Assignment[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (staff)
      api
        .staffAssignments(staff.id)
        .then((r) => setAssignments(r.assignments))
        .catch(onError);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [staff?.id]);

  const set = (k: keyof typeof form) => (e: { target: { value: string } }) =>
    setForm((f) => ({ ...f, [k]: e.target.value }));

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!form.full_name.trim()) {
      setError(t("common.required"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const body = {
        staff_number: form.staff_number.trim() || undefined,
        full_name: form.full_name.trim(),
        father_name: form.father_name,
        gender: form.gender,
        phone: form.phone.trim() || null,
        email: form.email.trim() || null,
        cnic: form.cnic.trim() || null,
        designation: form.designation,
        joined_on: form.joined_on || null,
        status: form.status as "active" | "left",
      };
      const saved = staff ? await api.updateStaff(staff.id, body) : await api.createStaff(body);
      await api.setStaffAssignments(
        saved.id,
        assignments.filter((a) => a.section_id),
      );
      onSaved();
    } catch (err) {
      setError(errorMessage(err, t("app.error.generic")));
    } finally {
      setBusy(false);
    }
  };

  const classTeacherOf = staff ? sections.filter((s) => s.class_teacher_id === staff.id) : [];

  return (
    <Dialog open onClose={onClose} title={staff ? t("staff.edit") : t("staff.add")} size="lg">
      <form onSubmit={save} className="flex flex-col gap-3">
        <div className="grid grid-cols-3 gap-3">
          <Input
            label={t("common.name")}
            value={form.full_name}
            onChange={set("full_name")}
            required
            autoFocus
            className="col-span-2"
            disabled={!manage}
          />
          <Input
            label={t("staff.number")}
            value={form.staff_number}
            onChange={set("staff_number")}
            disabled={!!staff || !manage}
            hint={staff ? undefined : t("field.optional")}
          />
          <Input
            label={t("staff.designation")}
            value={form.designation}
            onChange={set("designation")}
            disabled={!manage}
          />
          <Select
            label={t("common.gender")}
            value={form.gender}
            onChange={set("gender")}
            options={GENDERS.map((g) => ({ value: g, label: genderLabel(t, g) }))}
            disabled={!manage}
          />
          <Select
            label={t("common.status")}
            value={form.status}
            onChange={set("status")}
            options={[
              { value: "active", label: t("staff.status.active") },
              { value: "left", label: t("staff.status.left") },
            ]}
            disabled={!manage}
          />
          <Input
            label={t("common.phone")}
            value={form.phone}
            onChange={set("phone")}
            placeholder="0300-1234567"
            disabled={!manage}
          />
          <Input
            label={t("common.email")}
            value={form.email}
            onChange={set("email")}
            disabled={!manage}
          />
          <Input
            label={t("common.cnic")}
            value={form.cnic}
            onChange={set("cnic")}
            placeholder="42101-1234567-1"
            disabled={!manage}
          />
          <Input
            label={t("staff.joinedOn")}
            type="date"
            value={form.joined_on}
            onChange={set("joined_on")}
            disabled={!manage}
          />
        </div>
        {staff && <LoginSection staff={staff} onChanged={onSaved} />}
        {classTeacherOf.length > 0 && (
          <p className="text-xs text-muted">
            {t("staff.classTeacherOf")}:{" "}
            {classTeacherOf.map((s) => `${s.class_name}-${s.name}`).join(", ")}
          </p>
        )}
        <fieldset className="rounded border border-border p-3">
          <legend className="px-1 text-xs font-medium text-text">{t("staff.assignments")}</legend>
          <p className="mb-2 text-xs text-muted">{t("staff.assignments.hint")}</p>
          <div className="flex flex-col gap-2">
            {assignments.map((a, i) => (
              <div key={i} className="flex items-end gap-2">
                <Select
                  aria-label={t("common.section")}
                  value={a.section_id}
                  onChange={(e) =>
                    setAssignments(
                      assignments.map((x, j) =>
                        j === i ? { ...x, section_id: e.target.value } : x,
                      ),
                    )
                  }
                  options={[
                    { value: "", label: t("common.section") },
                    ...sections.map((s) => ({ value: s.id, label: `${s.class_name}-${s.name}` })),
                  ]}
                  className="w-40"
                  disabled={!manage}
                />
                <Select
                  aria-label={t("common.subject")}
                  value={a.subject_id ?? ""}
                  onChange={(e) =>
                    setAssignments(
                      assignments.map((x, j) =>
                        j === i ? { ...x, subject_id: e.target.value || null } : x,
                      ),
                    )
                  }
                  options={[
                    { value: "", label: t("common.subject") },
                    ...subjects.map((s) => ({ value: s.id, label: s.name })),
                  ]}
                  className="w-48"
                  disabled={!manage}
                />
                {manage && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="text-danger"
                    onClick={() => setAssignments(assignments.filter((_, j) => j !== i))}
                  >
                    {t("action.delete")}
                  </Button>
                )}
              </div>
            ))}
            {manage && (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                className="self-start"
                onClick={() =>
                  setAssignments([
                    ...assignments,
                    { section_id: sections[0]?.id ?? "", subject_id: null },
                  ])
                }
              >
                {t("staff.assignments.add")}
              </Button>
            )}
          </div>
        </fieldset>
        {error && (
          <p role="alert" className="rounded bg-danger-bg px-2.5 py-2 text-xs text-danger">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {manage ? t("action.cancel") : t("action.close")}
          </Button>
          {manage && (
            <Button type="submit" loading={busy}>
              {t("action.save")}
            </Button>
          )}
        </div>
      </form>
    </Dialog>
  );
}

const ROLES: LoginRole[] = ["principal", "admin", "accountant", "teacher"];

/** "Login account" block of the staff dialog: create, change role, enable/disable, reset. */
export function LoginSection({ staff, onChanged }: { staff: Staff; onChanged: () => void }) {
  const { t } = useI18n();
  const { can, user } = useSession();
  const toast = useToast();
  const manage = can("users.manage");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<LoginRole>((staff.roles[0] as LoginRole) ?? "teacher");
  const [creating, setCreating] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isSelf = staff.user_id !== null && staff.user_id === user?.id;

  const run = async (work: () => Promise<unknown>, done: string) => {
    setBusy(true);
    setError(null);
    try {
      await work();
      toast.push({ kind: "success", title: done });
      setCreating(false);
      setResetting(false);
      setPassword("");
      onChanged();
    } catch (err) {
      setError(errorMessage(err, t("app.error.generic")));
    } finally {
      setBusy(false);
    }
  };

  const roleOptions = ROLES.map((r) => ({ value: r, label: t(`staff.login.role.${r}`) }));

  return (
    <fieldset className="rounded border border-border p-3">
      <legend className="px-1 text-xs font-medium text-text">{t("staff.login")}</legend>
      {!staff.user_id ? (
        <div className="flex flex-col gap-2">
          {!creating ? (
            <div className="flex items-center gap-3">
              <span className="text-sm text-muted">{t("staff.login.none")}</span>
              {manage && (
                <Button size="sm" variant="secondary" onClick={() => setCreating(true)}>
                  {t("staff.login.create")}
                </Button>
              )}
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <p className="text-xs text-muted">{t("staff.login.hint")}</p>
              <div className="grid grid-cols-3 gap-3">
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
              </div>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  loading={busy}
                  onClick={() =>
                    void run(
                      () => api.createStaffLogin(staff.id, { username, password, role }),
                      t("staff.login.created"),
                    )
                  }
                >
                  {t("staff.login.create")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setCreating(false)}>
                  {t("action.cancel")}
                </Button>
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          <div className="grid grid-cols-3 items-end gap-3">
            <Input
              label={t("staff.login.username")}
              value={staff.username ?? ""}
              readOnly
              disabled
            />
            <Select
              label={t("staff.login.role")}
              value={role}
              onChange={(e) => {
                const next = e.target.value as LoginRole;
                setRole(next);
                void run(
                  () => api.updateStaffLogin(staff.id, { role: next }),
                  t("staff.login.saved"),
                );
              }}
              options={roleOptions}
              disabled={!manage || busy}
            />
            <label className="flex items-center gap-2 pb-2 text-sm">
              <input
                type="checkbox"
                checked={staff.login_active ?? false}
                disabled={!manage || busy || isSelf}
                onChange={(e) =>
                  void run(
                    () => api.updateStaffLogin(staff.id, { is_active: e.target.checked }),
                    t("staff.login.saved"),
                  )
                }
              />
              {staff.login_active === false ? (
                <Badge tone="warning">{t("staff.login.disabled")}</Badge>
              ) : (
                t("staff.login.active")
              )}
            </label>
          </div>
          {manage &&
            (!resetting ? (
              <div>
                <Button size="sm" variant="secondary" onClick={() => setResetting(true)}>
                  {t("staff.login.reset")}
                </Button>
              </div>
            ) : (
              <div className="flex items-end gap-2">
                <Input
                  label={t("staff.login.newPassword")}
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  autoComplete="new-password"
                  className="max-w-xs"
                />
                <Button
                  size="sm"
                  loading={busy}
                  onClick={() =>
                    void run(
                      () => api.resetStaffPassword(staff.id, password),
                      t("staff.login.resetDone"),
                    )
                  }
                >
                  {t("staff.login.reset")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setResetting(false)}>
                  {t("action.cancel")}
                </Button>
              </div>
            ))}
        </div>
      )}
      {error && (
        <p role="alert" className="mt-2 text-sm text-danger">
          {error}
        </p>
      )}
    </fieldset>
  );
}
