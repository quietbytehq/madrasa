import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { Trash2 } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";
import { ApiError } from "@/api/client";
import { api } from "@/api/endpoints";
import type {
  GuardianView,
  DuplicateMatch,
  Gender,
  GuardianInput,
  NewStudentRequest,
  Relationship,
  StudentDetail,
  StudentPatch,
} from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSchool } from "@/app/session";
import { placeholders } from "@/lib/school";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";
import { GENDERS, genderLabel, RELATIONSHIPS, relationshipLabel, today } from "./common";

export interface StudentFormValues {
  full_name: string;
  father_name: string;
  date_of_birth: string;
  gender: Gender;
  cnic: string;
  address: string;
  admission_date: string;
  student_number: string;
  legacy_id: string;
  board_roll_no: string;
  board_registration_no: string;
  section_id: string;
  roll_no: string;
  custom: Array<{ key: string; value: string }>;
}

export const EMPTY_FORM: StudentFormValues = {
  full_name: "",
  father_name: "",
  date_of_birth: "",
  gender: "male",
  cnic: "",
  address: "",
  admission_date: today(),
  student_number: "",
  legacy_id: "",
  board_roll_no: "",
  board_registration_no: "",
  section_id: "",
  roll_no: "",
  custom: [],
};

type Errors = Partial<Record<keyof StudentFormValues, TranslationKey>>;

export function validateStudent(f: StudentFormValues): Errors {
  const e: Errors = {};
  if (!f.full_name.trim()) e.full_name = "common.required";
  if (!f.father_name.trim()) e.father_name = "common.required";
  if (!f.date_of_birth) e.date_of_birth = "common.required";
  if (!f.admission_date) e.admission_date = "common.required";
  return e;
}

export function toCreateRequest(
  f: StudentFormValues,
  guardians: GuardianInput[],
  force: boolean,
): NewStudentRequest {
  const custom: Record<string, unknown> = {};
  for (const { key, value } of f.custom) if (key.trim()) custom[key.trim()] = value;
  return {
    full_name: f.full_name.trim(),
    father_name: f.father_name.trim(),
    date_of_birth: f.date_of_birth,
    gender: f.gender,
    cnic: f.cnic.trim() || undefined,
    address: f.address.trim() || undefined,
    admission_date: f.admission_date,
    student_number: f.student_number.trim() || undefined,
    legacy_id: f.legacy_id.trim() || undefined,
    board_roll_no: f.board_roll_no.trim() || undefined,
    board_registration_no: f.board_registration_no.trim() || undefined,
    section_id: f.section_id || undefined,
    roll_no: f.roll_no ? Number(f.roll_no) : undefined,
    custom,
    guardians,
    force,
  };
}

export function toPatch(f: StudentFormValues): StudentPatch {
  const custom: Record<string, unknown> = {};
  for (const { key, value } of f.custom) if (key.trim()) custom[key.trim()] = value;
  return {
    full_name: f.full_name.trim(),
    father_name: f.father_name.trim(),
    date_of_birth: f.date_of_birth,
    gender: f.gender,
    cnic: f.cnic.trim() || null,
    address: f.address.trim(),
    admission_date: f.admission_date,
    legacy_id: f.legacy_id.trim() || null,
    board_roll_no: f.board_roll_no.trim() || null,
    board_registration_no: f.board_registration_no.trim() || null,
    custom,
  };
}

export function fromDetail(d: StudentDetail): StudentFormValues {
  return {
    full_name: d.full_name,
    father_name: d.father_name,
    date_of_birth: d.date_of_birth,
    gender: d.gender,
    cnic: d.cnic ?? "",
    address: d.address,
    admission_date: d.admission_date,
    student_number: d.student_number,
    legacy_id: d.legacy_id ?? "",
    board_roll_no: d.board_roll_no ?? "",
    board_registration_no: d.board_registration_no ?? "",
    section_id: d.current?.section_id ?? "",
    roll_no: d.current?.roll_no?.toString() ?? "",
    custom: Object.entries(d.custom).map(([key, value]) => ({
      key,
      value: typeof value === "string" ? value : JSON.stringify(value),
    })),
  };
}

interface GuardianDraft extends GuardianInput {
  /** Display name of a linked existing guardian. */
  existing_name?: string;
}

/** The linked guardians of a student as editable drafts (edit form). */
export function guardianDrafts(d: StudentDetail): GuardianDraft[] {
  return d.guardians.map((g) => ({
    guardian_id: g.id,
    existing_name: g.full_name,
    full_name: g.full_name,
    relationship: g.relationship,
    phone: g.phone ?? "",
    email: g.email ?? "",
    cnic: g.cnic ?? "",
    occupation: g.occupation,
    address: g.address,
    is_primary: g.is_primary,
  }));
}

/** Fields of existing guardians the user changed, keyed by guardian id, ready to PATCH. */
export function guardianPatches(
  initial: GuardianDraft[],
  current: GuardianDraft[],
): Array<{ id: string; patch: Partial<GuardianView> }> {
  const out: Array<{ id: string; patch: Partial<GuardianView> }> = [];
  for (const g of current) {
    if (!g.guardian_id) continue;
    const before = initial.find((b) => b.guardian_id === g.guardian_id);
    if (!before) continue;
    const patch: Partial<GuardianView> = {};
    const text = (v: string | undefined) => (v ?? "").trim();
    if (text(g.full_name) !== text(before.full_name) && text(g.full_name))
      patch.full_name = text(g.full_name);
    for (const key of ["phone", "email", "cnic"] as const) {
      if (text(g[key]) !== text(before[key])) patch[key] = text(g[key]) || null;
    }
    for (const key of ["occupation", "address"] as const) {
      if (text(g[key]) !== text(before[key])) patch[key] = text(g[key]);
    }
    if (Object.keys(patch).length > 0) out.push({ id: g.guardian_id, patch });
  }
  return out;
}

/** Add (no id) or edit (id) a student. */
export function StudentFormPage() {
  const { id } = useParams();
  const editing = Boolean(id);
  const { t } = useI18n();
  const school = useSchool();
  const example = placeholders(school);
  const navigate = useNavigate();
  const toast = useToast();
  const existing = useAsync(() => (id ? api.student(id) : Promise.resolve(null)), [id]);
  const sections = useAsync(() => api.sections(), []);
  const [form, setForm] = useState<StudentFormValues>(EMPTY_FORM);
  const [errors, setErrors] = useState<Errors>({});
  const [guardians, setGuardians] = useState<GuardianDraft[]>([]);
  const [busy, setBusy] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [duplicates, setDuplicates] = useState<DuplicateMatch[] | null>(null);
  const [liveDuplicates, setLiveDuplicates] = useState<DuplicateMatch[]>([]);

  const [initialGuardians, setInitialGuardians] = useState<GuardianDraft[]>([]);
  useEffect(() => {
    if (existing.data) {
      setForm(fromDetail(existing.data));
      const drafts = guardianDrafts(existing.data);
      setInitialGuardians(drafts);
      setGuardians(drafts);
    }
  }, [existing.data]);

  // Inline duplicate warning while typing (debounced).
  useEffect(() => {
    if (!form.full_name.trim() || !form.father_name.trim() || !form.date_of_birth) {
      setLiveDuplicates([]);
      return;
    }
    const handle = setTimeout(() => {
      api
        .checkDuplicates({
          full_name: form.full_name,
          father_name: form.father_name,
          date_of_birth: form.date_of_birth,
          cnic: form.cnic || undefined,
          exclude_id: id,
        })
        .then(setLiveDuplicates)
        .catch(() => setLiveDuplicates([]));
    }, 400);
    return () => clearTimeout(handle);
  }, [form.full_name, form.father_name, form.date_of_birth, form.cnic, id]);

  const set = (key: keyof StudentFormValues) => (e: { target: { value: string } }) => {
    setForm((f) => ({ ...f, [key]: e.target.value }));
    setErrors((prev) => (prev[key] ? { ...prev, [key]: undefined } : prev));
  };

  const sectionOptions = useMemo(
    () => [
      { value: "", label: t("common.unassigned") },
      ...(sections.data ?? []).map((s) => ({ value: s.id, label: `${s.class_name}-${s.name}` })),
    ],
    [sections.data, t],
  );

  const submit = async (e: FormEvent, force = false) => {
    e.preventDefault();
    const errs = validateStudent(form);
    setErrors(errs);
    if (Object.values(errs).some(Boolean)) return;
    setBusy(true);
    setServerError(null);
    try {
      if (editing && id) {
        for (const { id: gid, patch } of guardianPatches(initialGuardians, guardians)) {
          await api.updateGuardian(gid, patch);
        }
        // Guardians added while editing (new records or links to existing ones).
        for (const g of guardians.filter(
          (d) => !initialGuardians.some((b) => b.guardian_id && b.guardian_id === d.guardian_id),
        )) {
          await api.linkGuardian(id, stripDraft(g));
        }
        let detail = await api.updateStudent(id, toPatch(form));
        const currentSection = existing.data?.current?.section_id ?? "";
        const currentRoll = existing.data?.current?.roll_no?.toString() ?? "";
        if (
          form.section_id &&
          (form.section_id !== currentSection || form.roll_no !== currentRoll)
        ) {
          detail = await api.enrolStudent(id, {
            section_id: form.section_id,
            roll_no: form.roll_no ? Number(form.roll_no) : null,
          });
        }
        toast.push({ kind: "success", title: t("students.updated") });
        navigate(`/students/${detail.id}`, { replace: true });
      } else {
        const detail = await api.createStudent(
          toCreateRequest(form, guardians.map(stripDraft), force),
        );
        toast.push({
          kind: "success",
          title: t("students.created", { number: detail.student_number }),
        });
        navigate(`/students/${detail.id}`, { replace: true });
      }
    } catch (err) {
      if (err instanceof ApiError && err.code === "possible_duplicate") {
        setDuplicates(
          ((err as ApiError & { details?: DuplicateMatch[] }).details as
            DuplicateMatch[] | undefined) ?? [],
        );
      } else {
        setServerError(errorMessage(err, t("app.error.generic")));
      }
    } finally {
      setBusy(false);
    }
  };

  if (editing && existing.loading && !existing.data) {
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  }

  const err = (k: keyof StudentFormValues) => (errors[k] ? t(errors[k]!) : undefined);
  const title = editing ? t("students.edit") : t("students.add");

  return (
    <>
      <PageHeader
        title={title}
        crumbs={[
          { label: t("nav.dashboard"), to: "/" },
          { label: t("students.title"), to: "/students" },
          { label: title },
        ]}
      />
      <form onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-4">
        {liveDuplicates.length > 0 && !editing && (
          <div
            role="status"
            className="rounded border border-warning bg-warning-bg px-3 py-2 text-xs text-text"
          >
            <strong>{t("students.duplicate.title")}:</strong>{" "}
            {liveDuplicates
              .map(
                (m) =>
                  `${m.student.full_name} (${m.student.student_number}) — ${t(`students.duplicate.reason.${m.reason}`)}`,
              )
              .join("; ")}
          </div>
        )}
        <div className="grid gap-4 xl:grid-cols-2">
          <Card title={t("students.tabs.details")}>
            <div className="grid grid-cols-2 gap-3">
              <Input
                label={t("students.fullName")}
                value={form.full_name}
                onChange={set("full_name")}
                error={err("full_name")}
                autoFocus
                required
                className="col-span-2"
              />
              <Input
                label={t("students.fatherName")}
                value={form.father_name}
                onChange={set("father_name")}
                error={err("father_name")}
                required
                className="col-span-2"
              />
              <Input
                label={t("students.dob")}
                type="date"
                value={form.date_of_birth}
                onChange={set("date_of_birth")}
                error={err("date_of_birth")}
                required
              />
              <Select
                label={t("common.gender")}
                value={form.gender}
                onChange={set("gender")}
                options={GENDERS.map((g) => ({ value: g, label: genderLabel(t, g) }))}
              />
              <Input
                label={school.country === "PK" ? t("common.bform") : school.id_label}
                value={form.cnic}
                onChange={set("cnic")}
                placeholder={example.id}
              />
              <Input
                label={t("students.admissionDate")}
                type="date"
                value={form.admission_date}
                onChange={set("admission_date")}
                error={err("admission_date")}
                required
              />
              <Input
                label={t("common.address")}
                value={form.address}
                onChange={set("address")}
                className="col-span-2"
              />
              <Select
                label={t("students.classSection")}
                value={form.section_id}
                onChange={set("section_id")}
                options={sectionOptions}
              />
              <Input
                label={t("students.rollNo")}
                type="number"
                min={1}
                value={form.roll_no}
                onChange={set("roll_no")}
              />
            </div>
          </Card>
          <Card title={t("students.number")}>
            <div className="grid grid-cols-2 gap-3">
              <Input
                label={t("students.number")}
                value={form.student_number}
                onChange={set("student_number")}
                disabled={editing}
                hint={editing ? undefined : t("field.optional")}
              />
              <Input
                label={t("students.legacyId")}
                labelSuffix={t("field.optional")}
                value={form.legacy_id}
                onChange={set("legacy_id")}
              />
              <Input
                label={t("students.boardRollNo")}
                labelSuffix={t("field.optional")}
                value={form.board_roll_no}
                onChange={set("board_roll_no")}
              />
              <Input
                label={t("students.boardRegNo")}
                labelSuffix={t("field.optional")}
                value={form.board_registration_no}
                onChange={set("board_registration_no")}
              />
            </div>
            <h3 className="mb-1 mt-4 text-xs font-semibold text-text">{t("students.custom")}</h3>
            <p className="mb-2 text-xs text-muted">{t("students.custom.hint")}</p>
            <CustomFields
              rows={form.custom}
              onChange={(custom) => setForm((f) => ({ ...f, custom }))}
            />
          </Card>
          <GuardiansEditor guardians={guardians} onChange={setGuardians} />
        </div>
        {serverError && (
          <p role="alert" className="rounded bg-danger-bg px-2.5 py-2 text-xs text-danger">
            {serverError}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={() => navigate(-1)} disabled={busy}>
            {t("action.cancel")}
          </Button>
          <Button type="submit" loading={busy}>
            {t("action.save")}
          </Button>
        </div>
      </form>

      <Dialog
        open={duplicates !== null}
        onClose={() => setDuplicates(null)}
        title={t("students.duplicate.title")}
        description={t("students.duplicate.body")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setDuplicates(null)}>
              {t("action.cancel")}
            </Button>
            <Button
              variant="danger"
              loading={busy}
              onClick={(e) => void submit(e, true).then(() => setDuplicates(null))}
            >
              {t("action.anyway")}
            </Button>
          </>
        }
      >
        <ul className="divide-y divide-border text-sm">
          {(duplicates ?? []).map((m) => (
            <li key={m.student.id} className="flex items-center gap-2 py-2">
              <span className="font-medium text-text">{m.student.full_name}</span>
              <span className="font-mono text-xs text-muted">{m.student.student_number}</span>
              <span className="flex-1" />
              <Badge tone="warning">{t(`students.duplicate.reason.${m.reason}`)}</Badge>
            </li>
          ))}
        </ul>
      </Dialog>
    </>
  );
}

function stripDraft(g: GuardianDraft): GuardianInput {
  const { existing_name: _n, ...rest } = g;
  return rest;
}

export function CustomFields({
  rows,
  onChange,
}: {
  rows: Array<{ key: string; value: string }>;
  onChange: (rows: Array<{ key: string; value: string }>) => void;
}) {
  const { t } = useI18n();
  const update = (i: number, patch: Partial<{ key: string; value: string }>) =>
    onChange(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  return (
    <div className="flex flex-col gap-2">
      {rows.map((r, i) => (
        <div key={i} className="flex items-end gap-2">
          <Input
            aria-label={t("students.custom.key")}
            placeholder={t("students.custom.key")}
            value={r.key}
            onChange={(e) => update(i, { key: e.target.value })}
            className="w-40"
          />
          <Input
            aria-label={t("students.custom.value")}
            placeholder={t("students.custom.value")}
            value={r.value}
            onChange={(e) => update(i, { value: e.target.value })}
            className="flex-1"
          />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-label={t("action.delete")}
            onClick={() => onChange(rows.filter((_, j) => j !== i))}
          >
            <Trash2 className="h-4 w-4" aria-hidden />
          </Button>
        </div>
      ))}
      <Button
        type="button"
        variant="secondary"
        size="sm"
        className="self-start"
        onClick={() => onChange([...rows, { key: "", value: "" }])}
      >
        {t("action.add")}
      </Button>
    </div>
  );
}

/** Guardians sub-form used when adding a student: new records or links to existing ones. */
export function GuardiansEditor({
  guardians,
  onChange,
}: {
  guardians: GuardianDraft[];
  onChange: (g: GuardianDraft[]) => void;
}) {
  const { t } = useI18n();
  const school = useSchool();
  const example = placeholders(school);
  const [search, setSearch] = useState("");
  const results = useAsync(
    () => (search.trim().length >= 2 ? api.searchGuardians(search.trim()) : Promise.resolve([])),
    [search],
  );
  const update = (i: number, patch: Partial<GuardianDraft>) =>
    onChange(guardians.map((g, j) => (j === i ? { ...g, ...patch } : g)));
  const add = (g: GuardianDraft) =>
    onChange([...guardians, { ...g, is_primary: guardians.length === 0 }]);

  return (
    <Card title={t("guardians.title")} className="xl:col-span-2">
      <div className="grid gap-3 xl:grid-cols-2">
        <div>
          <div className="mb-2 flex flex-col gap-1">
            <Input
              label={t("guardians.existing")}
              hint={t("guardians.existing.hint")}
              type="search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          {(results.data ?? []).length > 0 && (
            <ul className="max-h-40 divide-y divide-border overflow-auto rounded border border-border text-sm">
              {results.data!.map((g) => (
                <li key={g.id} className="flex items-center gap-2 px-2 py-1.5">
                  <span className="font-medium text-text">{g.full_name}</span>
                  <span className="text-xs text-muted">{g.phone ?? g.cnic ?? ""}</span>
                  {g.students.length > 0 && (
                    <span className="text-2xs text-muted">
                      ({t("guardians.children")}: {g.students.map((s) => s.full_name).join(", ")})
                    </span>
                  )}
                  <span className="flex-1" />
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    disabled={guardians.some((x) => x.guardian_id === g.id)}
                    onClick={() =>
                      add({ guardian_id: g.id, existing_name: g.full_name, relationship: "father" })
                    }
                  >
                    {t("action.link")}
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className="mt-2"
            onClick={() =>
              add({ relationship: guardians.length === 0 ? "father" : "mother", full_name: "" })
            }
          >
            {t("guardians.new")}
          </Button>
        </div>
        <div className="flex flex-col gap-3">
          {guardians.length === 0 && <p className="text-xs text-muted">{t("guardians.empty")}</p>}
          {guardians.map((g, i) => (
            <fieldset key={i} className="rounded border border-border p-3">
              <legend className="px-1 text-xs font-medium text-text">
                {g.existing_name ?? t("guardians.new")}
              </legend>
              <div className="grid grid-cols-2 gap-2">
                <Input
                  label={t("common.name")}
                  value={g.full_name ?? ""}
                  onChange={(e) => update(i, { full_name: e.target.value })}
                  className="col-span-2"
                  required
                />
                <Input
                  label={t("common.phone")}
                  value={g.phone ?? ""}
                  onChange={(e) => update(i, { phone: e.target.value })}
                  placeholder={example.phone}
                />
                <Input
                  label={school.country === "PK" ? t("common.cnic") : school.id_label}
                  value={g.cnic ?? ""}
                  onChange={(e) => update(i, { cnic: e.target.value })}
                  placeholder={example.id}
                />
                <Input
                  label={t("common.email")}
                  value={g.email ?? ""}
                  onChange={(e) => update(i, { email: e.target.value })}
                />
                <Input
                  label={t("guardians.occupation")}
                  value={g.occupation ?? ""}
                  onChange={(e) => update(i, { occupation: e.target.value })}
                />

                <Select
                  label={t("guardians.relationship")}
                  value={g.relationship}
                  onChange={(e) => update(i, { relationship: e.target.value as Relationship })}
                  options={RELATIONSHIPS.map((r) => ({ value: r, label: relationshipLabel(t, r) }))}
                />
                <label className="flex items-center gap-2 self-end pb-2 text-sm text-text">
                  <input
                    type="checkbox"
                    className="h-4 w-4 accent-blue"
                    checked={!!g.is_primary}
                    onChange={(e) =>
                      onChange(
                        guardians.map((x, j) => ({
                          ...x,
                          is_primary:
                            j === i ? e.target.checked : e.target.checked ? false : x.is_primary,
                        })),
                      )
                    }
                  />
                  {t("guardians.primary")}
                </label>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="mt-1 text-danger"
                onClick={() => onChange(guardians.filter((_, j) => j !== i))}
              >
                {t("action.delete")}
              </Button>
            </fieldset>
          ))}
        </div>
      </div>
    </Card>
  );
}
