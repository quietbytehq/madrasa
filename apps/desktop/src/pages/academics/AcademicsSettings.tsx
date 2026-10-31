import { useMemo, useState } from "react";
import type { FormEvent } from "react";
import { Plus } from "lucide-react";
import { api } from "@/api/endpoints";
import type { AcademicYear, SchoolClass, Section, Staff, Subject } from "@/api/types";
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
import { RolloverDialog } from "./RolloverDialog";

const DEFAULT_CLASSES = ["KG", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];
const DEFAULT_SUBJECTS: Array<[string, string]> = [
  ["English", "ENG"],
  ["Urdu", "URD"],
  ["Mathematics", "MATH"],
  ["Science", "SCI"],
  ["Islamiat", "ISL"],
  ["Pakistan Studies", "PST"],
  ["Computer", "COMP"],
];

export function AcademicsSettings() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("academics.manage");
  const toast = useToast();
  const years = useAsync(() => api.academicYears(), []);
  const classes = useAsync(() => api.classes(), []);
  const subjects = useAsync(() => api.subjects(), []);
  const staff = useAsync(() => api.staff(), []);
  const activeYear = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const [yearId, setYearId] = useState<string | null>(null);
  const selectedYear = yearId ?? activeYear?.id ?? null;
  const sections = useAsync(
    () =>
      selectedYear
        ? api.sections({ academic_year_id: selectedYear })
        : Promise.resolve([] as Section[]),
    [selectedYear],
  );

  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });

  if (years.loading && !years.data) {
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  }

  return (
    <div className="grid gap-4 xl:grid-cols-2">
      <YearsCard
        years={years.data ?? []}
        manage={manage}
        onChange={() => void years.reload()}
        onError={fail}
      />
      <ClassesCard
        classes={classes.data ?? []}
        subjects={subjects.data ?? []}
        manage={manage}
        onChange={() => void classes.reload()}
        onError={fail}
      />
      <SectionsCard
        years={years.data ?? []}
        selectedYear={selectedYear}
        onSelectYear={setYearId}
        sections={sections.data ?? []}
        classes={classes.data ?? []}
        staff={staff.data ?? []}
        manage={manage}
        onChange={() => void sections.reload()}
        onError={fail}
      />
      <SubjectsCard
        subjects={subjects.data ?? []}
        manage={manage}
        onChange={() => void subjects.reload()}
        onError={fail}
      />
    </div>
  );
}

interface CardProps {
  manage: boolean;
  onChange: () => void;
  onError: (err: unknown) => void;
}

function YearsCard({ years, manage, onChange, onError }: CardProps & { years: AcademicYear[] }) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [rollover, setRollover] = useState(false);
  const [startYear, setStartYear] = useState(String(new Date().getFullYear()));
  const [startMonth, setStartMonth] = useState<"4" | "8">("4");
  const [busy, setBusy] = useState(false);

  const create = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api.createAcademicYear({
        start_year: Number(startYear),
        start_month: startMonth === "8" ? 8 : 4,
        activate: years.length === 0,
      });
      setOpen(false);
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  const activate = async (y: AcademicYear) => {
    if (!window.confirm(t("academics.years.activateConfirm", { name: y.name }))) return;
    try {
      await api.activateAcademicYear(y.id);
      onChange();
    } catch (err) {
      onError(err);
    }
  };

  return (
    <Card
      title={t("academics.years")}
      actions={
        manage && (
          <div className="flex gap-2">
            {years.some((y) => y.is_active) && (
              <Button size="sm" variant="secondary" onClick={() => setRollover(true)}>
                {t("rollover.button")}
              </Button>
            )}
            <Button
              size="sm"
              icon={<Plus className="h-3.5 w-3.5" aria-hidden />}
              onClick={() => setOpen(true)}
            >
              {t("academics.years.new")}
            </Button>
          </div>
        )
      }
      padded={false}
    >
      {years.length === 0 ? (
        <EmptyState title={t("academics.years")} description={t("academics.years.empty")} />
      ) : (
        <ul className="divide-y divide-border">
          {years.map((y) => (
            <li key={y.id} className="flex items-center gap-3 px-4 py-2 text-sm">
              <span className="font-medium text-text">{y.name}</span>
              <span className="text-xs text-muted">
                {formatDate(y.start_date)} – {formatDate(y.end_date)}
              </span>
              <span className="flex-1" />
              {y.is_active ? (
                <Badge tone="success">{t("academics.years.activeBadge")}</Badge>
              ) : (
                manage && (
                  <Button size="sm" variant="secondary" onClick={() => void activate(y)}>
                    {t("action.activate")}
                  </Button>
                )
              )}
            </li>
          ))}
        </ul>
      )}
      {rollover && (
        <RolloverDialog
          onClose={() => setRollover(false)}
          onDone={(message) => {
            setRollover(false);
            toast.push({ kind: "success", title: message });
            onChange();
          }}
          onError={onError}
        />
      )}
      <Dialog open={open} onClose={() => setOpen(false)} title={t("academics.years.new")} size="sm">
        <form onSubmit={create} className="flex flex-col gap-3">
          <Input
            label={t("academics.years.startYear")}
            type="number"
            min={2000}
            max={2100}
            value={startYear}
            onChange={(e) => setStartYear(e.target.value)}
            required
          />
          <Select
            label={t("academics.years.startMonth")}
            value={startMonth}
            onChange={(e) => setStartMonth(e.target.value as "4" | "8")}
            options={[
              { value: "4", label: t("setup.year.april") },
              { value: "8", label: t("setup.year.august") },
            ]}
          />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setOpen(false)}>
              {t("action.cancel")}
            </Button>
            <Button type="submit" loading={busy}>
              {t("action.create")}
            </Button>
          </div>
        </form>
      </Dialog>
    </Card>
  );
}

function ClassesCard({
  classes,
  subjects,
  manage,
  onChange,
  onError,
}: CardProps & { classes: SchoolClass[]; subjects: Subject[] }) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<SchoolClass | null>(null);
  const [editName, setEditName] = useState("");
  const [editLevel, setEditLevel] = useState("0");
  const [editSubjects, setEditSubjects] = useState<string[]>([]);

  const add = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    setBusy(true);
    try {
      await api.createClass({ name: name.trim(), level: classes.length });
      setName("");
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  const addDefaults = async () => {
    setBusy(true);
    try {
      const existing = new Set(classes.map((c) => c.name.toLowerCase()));
      for (const [i, n] of DEFAULT_CLASSES.entries()) {
        if (!existing.has(n.toLowerCase())) await api.createClass({ name: n, level: i });
      }
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  const openEdit = async (c: SchoolClass) => {
    setEditing(c);
    setEditName(c.name);
    setEditLevel(String(c.level));
    try {
      const res = await api.classSubjects(c.id);
      setEditSubjects(res.subject_ids);
    } catch (err) {
      onError(err);
    }
  };

  const saveEdit = async (e: FormEvent) => {
    e.preventDefault();
    if (!editing) return;
    setBusy(true);
    try {
      await api.updateClass(editing.id, { name: editName.trim(), level: Number(editLevel) });
      await api.setClassSubjects(editing.id, editSubjects);
      setEditing(null);
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      title={t("academics.classes")}
      actions={
        manage &&
        classes.length === 0 && (
          <Button size="sm" variant="secondary" onClick={() => void addDefaults()} loading={busy}>
            {t("academics.classes.addDefaults")}
          </Button>
        )
      }
      padded={false}
    >
      {classes.length === 0 ? (
        <EmptyState title={t("academics.classes")} description={t("academics.classes.empty")} />
      ) : (
        <ul className="flex flex-wrap gap-2 p-3">
          {classes.map((c) => (
            <li key={c.id}>
              <button
                type="button"
                disabled={!manage}
                onClick={() => void openEdit(c)}
                className="rounded border border-border bg-bg px-2.5 py-1 text-sm text-text hover:bg-hover disabled:cursor-default"
              >
                {c.name}
              </button>
            </li>
          ))}
        </ul>
      )}
      {manage && (
        <form onSubmit={add} className="flex items-end gap-2 border-t border-border p-3">
          <Input
            label={t("academics.classes.new")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="max-w-xs"
          />
          <Button type="submit" loading={busy} disabled={!name.trim()}>
            {t("action.add")}
          </Button>
        </form>
      )}
      <Dialog
        open={!!editing}
        onClose={() => setEditing(null)}
        title={editing?.name ?? ""}
        size="sm"
      >
        <form onSubmit={saveEdit} className="flex flex-col gap-3">
          <Input
            label={t("common.name")}
            value={editName}
            onChange={(e) => setEditName(e.target.value)}
            required
          />
          <Input
            label={t("academics.classes.level")}
            type="number"
            value={editLevel}
            onChange={(e) => setEditLevel(e.target.value)}
            className="max-w-32"
          />
          <fieldset>
            <legend className="mb-1 text-xs font-medium text-text">
              {t("academics.subjects.forClass", { class: editing?.name ?? "" })}
            </legend>
            {subjects.length === 0 ? (
              <p className="text-xs text-muted">{t("academics.subjects.empty")}</p>
            ) : (
              <div className="grid grid-cols-2 gap-1">
                {subjects.map((s) => (
                  <label key={s.id} className="flex items-center gap-2 text-sm text-text">
                    <input
                      type="checkbox"
                      className="h-4 w-4 accent-blue"
                      checked={editSubjects.includes(s.id)}
                      onChange={(e) =>
                        setEditSubjects((prev) =>
                          e.target.checked ? [...prev, s.id] : prev.filter((x) => x !== s.id),
                        )
                      }
                    />
                    {s.name}
                  </label>
                ))}
              </div>
            )}
          </fieldset>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setEditing(null)}>
              {t("action.cancel")}
            </Button>
            <Button type="submit" loading={busy}>
              {t("action.save")}
            </Button>
          </div>
        </form>
      </Dialog>
    </Card>
  );
}

function SectionsCard({
  years,
  selectedYear,
  onSelectYear,
  sections,
  classes,
  staff,
  manage,
  onChange,
  onError,
}: CardProps & {
  years: AcademicYear[];
  selectedYear: string | null;
  onSelectYear: (id: string) => void;
  sections: Section[];
  classes: SchoolClass[];
  staff: Staff[];
}) {
  const { t } = useI18n();
  const [editing, setEditing] = useState<Section | "new" | null>(null);
  const [classId, setClassId] = useState("");
  const [name, setName] = useState("");
  const [teacher, setTeacher] = useState("");
  const [capacity, setCapacity] = useState("");
  const [busy, setBusy] = useState(false);

  const openNew = () => {
    setEditing("new");
    setClassId(classes[0]?.id ?? "");
    setName("");
    setTeacher("");
    setCapacity("");
  };
  const openEdit = (s: Section) => {
    setEditing(s);
    setClassId(s.class_id);
    setName(s.name);
    setTeacher(s.class_teacher_id ?? "");
    setCapacity(s.capacity?.toString() ?? "");
  };

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!selectedYear || !editing) return;
    setBusy(true);
    try {
      const body = {
        name: name.trim(),
        class_teacher_id: teacher || null,
        capacity: capacity ? Number(capacity) : null,
      };
      if (editing === "new")
        await api.createSection({ class_id: classId, academic_year_id: selectedYear, ...body });
      else await api.updateSection(editing.id, body);
      setEditing(null);
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  const teacherOptions = [
    { value: "", label: t("common.unassigned") },
    ...staff.filter((s) => s.status === "active").map((s) => ({ value: s.id, label: s.full_name })),
  ];
  const title =
    editing === "new"
      ? t("academics.sections.new")
      : editing
        ? `${editing.class_name}-${editing.name}`
        : "";

  return (
    <Card
      title={t("academics.sections")}
      actions={
        <>
          <Select
            aria-label={t("academics.sections.year")}
            value={selectedYear ?? ""}
            onChange={(e) => onSelectYear(e.target.value)}
            options={years.map((y) => ({
              value: y.id,
              label: y.is_active ? `${y.name} ★` : y.name,
            }))}
            className="w-40"
          />
          {manage && (
            <Button
              size="sm"
              icon={<Plus className="h-3.5 w-3.5" aria-hidden />}
              onClick={openNew}
              disabled={!selectedYear || classes.length === 0}
            >
              {t("academics.sections.new")}
            </Button>
          )}
        </>
      }
      padded={false}
    >
      {sections.length === 0 ? (
        <EmptyState title={t("academics.sections")} description={t("academics.sections.empty")} />
      ) : (
        <Table
          rows={sections}
          rowKey={(s) => s.id}
          pageSize={0}
          onRowClick={manage ? openEdit : undefined}
          className="[&>div]:rounded-none [&>div]:border-0"
          columns={[
            {
              key: "class_name",
              header: t("common.class"),
              accessor: (s) => s.class_level,
              render: (s) => s.class_name,
            },
            { key: "name", header: t("common.section") },
            {
              key: "class_teacher_name",
              header: t("academics.sections.classTeacher"),
              render: (s) =>
                s.class_teacher_name ?? (
                  <span className="text-muted">{t("common.unassigned")}</span>
                ),
            },
            { key: "student_count", header: t("academics.sections.students"), align: "end" },
            {
              key: "capacity",
              header: t("academics.sections.capacity"),
              align: "end",
              render: (s) => s.capacity ?? "—",
            },
          ]}
        />
      )}
      <Dialog open={!!editing} onClose={() => setEditing(null)} title={title} size="sm">
        <form onSubmit={save} className="flex flex-col gap-3">
          {editing === "new" && (
            <Select
              label={t("common.class")}
              value={classId}
              onChange={(e) => setClassId(e.target.value)}
              options={classes.map((c) => ({ value: c.id, label: c.name }))}
            />
          )}
          <Input
            label={t("common.section")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            className="max-w-32"
          />
          <Select
            label={t("academics.sections.classTeacher")}
            value={teacher}
            onChange={(e) => setTeacher(e.target.value)}
            options={teacherOptions}
          />
          <Input
            label={t("academics.sections.capacity")}
            type="number"
            min={1}
            value={capacity}
            onChange={(e) => setCapacity(e.target.value)}
            className="max-w-32"
          />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setEditing(null)}>
              {t("action.cancel")}
            </Button>
            <Button type="submit" loading={busy}>
              {t("action.save")}
            </Button>
          </div>
        </form>
      </Dialog>
    </Card>
  );
}

function SubjectsCard({
  subjects,
  manage,
  onChange,
  onError,
}: CardProps & { subjects: Subject[] }) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);

  const add = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api.createSubject({ name: name.trim(), code: code.trim() || name.trim().slice(0, 4) });
      setName("");
      setCode("");
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  const addDefaults = async () => {
    setBusy(true);
    try {
      const existing = new Set(subjects.map((s) => s.code.toUpperCase()));
      for (const [n, c] of DEFAULT_SUBJECTS) {
        if (!existing.has(c)) await api.createSubject({ name: n, code: c });
      }
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      title={t("academics.subjects")}
      actions={
        manage &&
        subjects.length === 0 && (
          <Button size="sm" variant="secondary" onClick={() => void addDefaults()} loading={busy}>
            {t("academics.subjects.addDefaults")}
          </Button>
        )
      }
      padded={false}
    >
      {subjects.length === 0 ? (
        <EmptyState title={t("academics.subjects")} description={t("academics.subjects.empty")} />
      ) : (
        <ul className="flex flex-wrap gap-2 p-3">
          {subjects.map((s) => (
            <li
              key={s.id}
              className="rounded border border-border bg-bg px-2.5 py-1 text-sm text-text"
            >
              {s.name} <span className="text-xs text-muted">{s.code}</span>
            </li>
          ))}
        </ul>
      )}
      {manage && (
        <form onSubmit={add} className="flex items-end gap-2 border-t border-border p-3">
          <Input
            label={t("academics.subjects.new")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="max-w-xs"
          />
          <Input
            label={t("common.code")}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            className="w-28"
          />
          <Button type="submit" loading={busy} disabled={!name.trim()}>
            {t("action.add")}
          </Button>
        </form>
      )}
    </Card>
  );
}
