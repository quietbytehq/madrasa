import { useRef, useState } from "react";
import type { FormEvent } from "react";
import { Pencil, UserRound } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { getBaseUrl, getToken } from "@/api/client";
import { api } from "@/api/endpoints";
import type { GuardianInput, Relationship, StudentDetail, StudentStatus } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { cn } from "@/lib/cn";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";
import { PageHeader } from "../PageHeader";
import {
  classLabel,
  genderLabel,
  RELATIONSHIPS,
  relationshipLabel,
  StatusBadge,
  today,
} from "./common";

const TABS: Array<{ key: string; labelKey: TranslationKey; available: boolean }> = [
  { key: "details", labelKey: "students.tabs.details", available: true },
  { key: "guardians", labelKey: "students.tabs.guardians", available: true },
  { key: "fees", labelKey: "students.tabs.fees", available: false },
  { key: "attendance", labelKey: "students.tabs.attendance", available: false },
  { key: "results", labelKey: "students.tabs.results", available: false },
];

export function StudentProfile() {
  const { id = "" } = useParams();
  const { t } = useI18n();
  const { can } = useSession();
  const navigate = useNavigate();
  const toast = useToast();
  const student = useAsync(() => api.student(id), [id]);
  const [tab, setTab] = useState("details");
  const [withdrawOpen, setWithdrawOpen] = useState(false);
  const manage = can("students.manage");

  if (student.loading && !student.data) {
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  }
  if (student.error || !student.data) {
    return (
      <EmptyState
        title={errorMessage(student.error, t("common.error.load"))}
        action={
          <Button variant="secondary" onClick={() => navigate("/students")}>
            {t("action.back")}
          </Button>
        }
      />
    );
  }
  const s = student.data;
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });

  return (
    <>
      <PageHeader
        title={s.full_name}
        crumbs={[
          { label: t("nav.dashboard"), to: "/" },
          { label: t("students.title"), to: "/students" },
          { label: s.student_number },
        ]}
        actions={
          manage && (
            <>
              {s.status === "active" ? (
                <Button variant="secondary" onClick={() => setWithdrawOpen(true)}>
                  {t("students.withdraw")}
                </Button>
              ) : (
                <Button variant="secondary" onClick={() => setWithdrawOpen(true)}>
                  {t("students.readmit")}
                </Button>
              )}
              <Button
                icon={<Pencil className="h-4 w-4" aria-hidden />}
                onClick={() => navigate(`/students/${s.id}/edit`)}
              >
                {t("action.edit")}
              </Button>
            </>
          )
        }
      />
      <div className="mb-3 flex items-center gap-4">
        <Photo
          student={s}
          canEdit={manage}
          onChanged={() => void student.reload()}
          onError={fail}
        />
        <div className="text-sm">
          <div className="flex items-center gap-2">
            <span className="font-mono text-muted">{s.student_number}</span>
            <StatusBadge status={s.status} t={t} />
          </div>
          <div className="text-text">
            {t("students.fatherName")}: {s.father_name}
          </div>
          <div className="text-muted">
            {t("students.classSection")}:{" "}
            {s.current
              ? classLabel(s.current.class_name, s.current.section_name)
              : t("students.notEnrolled")}
            {s.current?.roll_no != null && ` · ${t("students.rollNo")} ${s.current.roll_no}`}
          </div>
        </div>
      </div>

      <div
        role="tablist"
        aria-label={s.full_name}
        className="mb-3 flex gap-1 border-b border-border"
      >
        {TABS.map((tb) => (
          <button
            key={tb.key}
            role="tab"
            type="button"
            aria-selected={tab === tb.key}
            onClick={() => setTab(tb.key)}
            className={cn(
              "-mb-px flex items-center gap-1.5 border-b-2 px-3 py-2 text-sm",
              tab === tb.key
                ? "border-blue font-medium text-blue"
                : "border-transparent text-muted hover:text-text",
            )}
          >
            {t(tb.labelKey)}
            {!tb.available && <Badge tone="warning">{t("app.comingSoon")}</Badge>}
          </button>
        ))}
      </div>

      <div role="tabpanel">
        {tab === "details" && <DetailsTab s={s} />}
        {tab === "guardians" && (
          <GuardiansTab
            s={s}
            manage={manage}
            onChanged={() => void student.reload()}
            onError={fail}
          />
        )}
        {!TABS.find((x) => x.key === tab)?.available && (
          <Card>
            <EmptyState title={t("app.comingSoon")} description={t("app.comingSoonBody")} />
          </Card>
        )}
      </div>

      <WithdrawDialog
        open={withdrawOpen}
        student={s}
        onClose={() => setWithdrawOpen(false)}
        onDone={(d) => {
          student.setData(d);
          setWithdrawOpen(false);
        }}
        onError={fail}
      />
    </>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <dt className="text-2xs uppercase tracking-wide text-muted">{label}</dt>
      <dd className="text-sm text-text">{children ?? "—"}</dd>
    </div>
  );
}

function DetailsTab({ s }: { s: StudentDetail }) {
  const { t, formatDate } = useI18n();
  const custom = Object.entries(s.custom);
  return (
    <div className="grid gap-4 xl:grid-cols-2">
      <Card title={t("students.tabs.details")}>
        <dl className="grid grid-cols-2 gap-3">
          <Field label={t("students.dob")}>{formatDate(s.date_of_birth)}</Field>
          <Field label={t("common.gender")}>{genderLabel(t, s.gender)}</Field>
          <Field label={t("common.bform")}>{s.cnic ?? "—"}</Field>
          <Field label={t("students.admissionDate")}>{formatDate(s.admission_date)}</Field>
          <Field label={t("common.address")}>{s.address || "—"}</Field>
          <Field label={t("students.legacyId")}>{s.legacy_id ?? "—"}</Field>
          <Field label={t("students.boardRollNo")}>{s.board_roll_no ?? "—"}</Field>
          <Field label={t("students.boardRegNo")}>{s.board_registration_no ?? "—"}</Field>
          {s.status !== "active" && (
            <>
              <Field label={t("students.leftOn")}>{s.left_on ? formatDate(s.left_on) : "—"}</Field>
              <Field label={t("students.leftReason")}>{s.left_reason ?? "—"}</Field>
            </>
          )}
          {custom.map(([k, v]) => (
            <Field key={k} label={k}>
              {typeof v === "string" ? v : JSON.stringify(v)}
            </Field>
          ))}
        </dl>
      </Card>
      <div className="flex flex-col gap-4">
        <Card title={t("students.enrollments")} padded={false}>
          {s.enrollments.length === 0 ? (
            <EmptyState title={t("students.notEnrolled")} />
          ) : (
            <ul className="divide-y divide-border text-sm">
              {s.enrollments.map((e) => (
                <li key={e.id} className="flex items-center gap-3 px-4 py-2">
                  <span className="text-muted">{e.academic_year_name}</span>
                  <span className="font-medium text-text">
                    {classLabel(e.class_name, e.section_name)}
                  </span>
                  {e.roll_no != null && <span className="text-xs text-muted">#{e.roll_no}</span>}
                  {s.current?.id === e.id && <Badge tone="info">{t("common.active")}</Badge>}
                </li>
              ))}
            </ul>
          )}
        </Card>
        <Card title={t("students.siblings")} padded={false}>
          {s.siblings.length === 0 ? (
            <EmptyState title={t("students.siblings")} description={t("students.siblings.empty")} />
          ) : (
            <ul className="divide-y divide-border text-sm">
              {s.siblings.map((sib) => (
                <li key={sib.id} className="px-4 py-2">
                  <Link
                    to={`/students/${sib.id}`}
                    className="font-medium text-blue hover:underline"
                  >
                    {sib.full_name}
                  </Link>
                  <span className="ms-2 text-xs text-muted">
                    {classLabel(sib.class_name, sib.section_name)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </div>
  );
}

function GuardiansTab({
  s,
  manage,
  onChanged,
  onError,
}: {
  s: StudentDetail;
  manage: boolean;
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState<GuardianInput>({ relationship: "father", full_name: "" });
  const [busy, setBusy] = useState(false);

  const unlink = async (guardianId: string, name: string) => {
    if (!window.confirm(t("guardians.unlinkConfirm", { name }))) return;
    try {
      await api.unlinkGuardian(s.id, guardianId);
      onChanged();
    } catch (err) {
      onError(err);
    }
  };

  const add = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api.linkGuardian(s.id, draft);
      setAdding(false);
      setDraft({ relationship: "father", full_name: "" });
      onChanged();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      title={t("guardians.title")}
      padded={false}
      actions={
        manage && (
          <Button size="sm" onClick={() => setAdding(true)}>
            {t("guardians.add")}
          </Button>
        )
      }
    >
      {s.guardians.length === 0 ? (
        <EmptyState title={t("guardians.empty")} />
      ) : (
        <ul className="divide-y divide-border text-sm">
          {s.guardians.map((g) => (
            <li key={g.id} className="flex flex-wrap items-center gap-x-4 gap-y-1 px-4 py-2">
              <span className="font-medium text-text">{g.full_name}</span>
              <Badge tone="info">{relationshipLabel(t, g.relationship)}</Badge>
              {g.is_primary && <Badge tone="success">{t("guardians.primary")}</Badge>}
              <span className="text-muted">{g.phone ?? ""}</span>
              <span className="text-muted">{g.cnic ?? ""}</span>
              <span className="text-muted">{g.occupation}</span>
              <span className="flex-1" />
              {manage && (
                <Button
                  size="sm"
                  variant="ghost"
                  className="text-danger"
                  onClick={() => void unlink(g.id, g.full_name)}
                >
                  {t("action.delete")}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <Dialog open={adding} onClose={() => setAdding(false)} title={t("guardians.add")} size="sm">
        <form onSubmit={add} className="flex flex-col gap-3">
          <Input
            label={t("common.name")}
            value={draft.full_name ?? ""}
            onChange={(e) => setDraft({ ...draft, full_name: e.target.value })}
            required
            autoFocus
          />
          <Select
            label={t("guardians.relationship")}
            value={draft.relationship}
            onChange={(e) => setDraft({ ...draft, relationship: e.target.value as Relationship })}
            options={RELATIONSHIPS.map((r) => ({ value: r, label: relationshipLabel(t, r) }))}
          />
          <Input
            label={t("common.phone")}
            value={draft.phone ?? ""}
            onChange={(e) => setDraft({ ...draft, phone: e.target.value })}
            placeholder="0300-1234567"
          />
          <Input
            label={t("common.cnic")}
            value={draft.cnic ?? ""}
            onChange={(e) => setDraft({ ...draft, cnic: e.target.value })}
            placeholder="42101-1234567-1"
          />
          <Input
            label={t("guardians.occupation")}
            value={draft.occupation ?? ""}
            onChange={(e) => setDraft({ ...draft, occupation: e.target.value })}
          />
          <label className="flex items-center gap-2 text-sm text-text">
            <input
              type="checkbox"
              className="h-4 w-4 accent-blue"
              checked={!!draft.is_primary}
              onChange={(e) => setDraft({ ...draft, is_primary: e.target.checked })}
            />
            {t("guardians.primary")}
          </label>
          <p className="text-xs text-muted">{t("guardians.existing.hint")}</p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setAdding(false)}>
              {t("action.cancel")}
            </Button>
            <Button type="submit" loading={busy}>
              {t("action.link")}
            </Button>
          </div>
        </form>
      </Dialog>
    </Card>
  );
}

function Photo({
  student,
  canEdit,
  onChanged,
  onError,
}: {
  student: StudentDetail;
  canEdit: boolean;
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [src, setSrc] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useAsync(async () => {
    if (!student.has_photo) {
      setSrc(null);
      return null;
    }
    const base = await getBaseUrl();
    const res = await fetch(`${base}/api/v1/students/${student.id}/photo`, {
      headers: { Authorization: `Bearer ${getToken() ?? ""}` },
    });
    if (!res.ok) throw new Error("photo");
    const blob = await res.blob();
    setSrc(URL.createObjectURL(blob));
    return null;
  }, [student.id, student.has_photo, version]);

  const upload = async (file: File) => {
    if (!["image/jpeg", "image/png", "image/webp"].includes(file.type))
      return onError(new Error(t("students.photo.badType")));
    if (file.size > 512 * 1024) return onError(new Error(t("students.photo.tooLarge")));
    const buf = new Uint8Array(await file.arrayBuffer());
    let binary = "";
    for (const b of buf) binary += String.fromCharCode(b);
    try {
      await api.setStudentPhoto(student.id, { mime: file.type, data: btoa(binary) });
      setVersion((v) => v + 1);
      onChanged();
    } catch (err) {
      onError(err);
    }
  };

  const remove = async () => {
    try {
      await api.deleteStudentPhoto(student.id);
      setSrc(null);
      onChanged();
    } catch (err) {
      onError(err);
    }
  };

  return (
    <div className="flex items-center gap-2">
      <div className="flex h-20 w-20 items-center justify-center overflow-hidden rounded-lg border border-border bg-lavender text-navy">
        {src ? (
          <img src={src} alt={t("students.photo")} className="h-full w-full object-cover" />
        ) : (
          <UserRound className="h-8 w-8" aria-hidden />
        )}
      </div>
      {canEdit && (
        <div className="flex flex-col gap-1">
          <input
            ref={input}
            type="file"
            accept="image/jpeg,image/png,image/webp"
            className="hidden"
            aria-label={t("students.photo.change")}
            onChange={(e) => e.target.files?.[0] && void upload(e.target.files[0])}
          />
          <Button size="sm" variant="secondary" onClick={() => input.current?.click()}>
            {t("students.photo.change")}
          </Button>
          {student.has_photo && (
            <Button size="sm" variant="ghost" className="text-danger" onClick={() => void remove()}>
              {t("students.photo.remove")}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

export function WithdrawDialog({
  open,
  student,
  onClose,
  onDone,
  onError,
}: {
  open: boolean;
  student: StudentDetail;
  onClose: () => void;
  onDone: (d: StudentDetail) => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const readmit = student.status !== "active";
  const [status, setStatus] = useState<StudentStatus>("left");
  const [leftOn, setLeftOn] = useState(today());
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!readmit && !reason.trim()) {
      setError(t("common.required"));
      return;
    }
    setBusy(true);
    try {
      const d = readmit
        ? await api.withdrawStudent(student.id, { status: "active" })
        : await api.withdrawStudent(student.id, { status, left_on: leftOn, reason: reason.trim() });
      onDone(d);
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={readmit ? t("students.readmit") : t("students.withdraw.title")}
      description={readmit ? undefined : t("students.withdraw.body")}
      size="sm"
    >
      <form onSubmit={submit} noValidate className="flex flex-col gap-3">
        {!readmit && (
          <>
            <Select
              label={t("students.withdraw.newStatus")}
              value={status}
              onChange={(e) => setStatus(e.target.value as StudentStatus)}
              options={[
                { value: "left", label: t("students.status.left") },
                { value: "graduated", label: t("students.status.graduated") },
              ]}
            />
            <Input
              label={t("students.withdraw.leftOn")}
              type="date"
              value={leftOn}
              onChange={(e) => setLeftOn(e.target.value)}
              required
            />
            <Input
              label={t("students.withdraw.reason")}
              value={reason}
              onChange={(e) => {
                setReason(e.target.value);
                setError(null);
              }}
              error={error ?? undefined}
              required
            />
          </>
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button type="submit" variant={readmit ? "primary" : "danger"} loading={busy}>
            {readmit ? t("students.readmit") : t("students.withdraw")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
