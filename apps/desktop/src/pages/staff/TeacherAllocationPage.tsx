import { useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import type { GridSection, TeacherGrid } from "@/api/types";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useSession } from "@/app/session";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";

/** Teacher allocation grid (M2 stage 7): sections down, subjects across, one teacher per cell. */
export function TeacherAllocationPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const toast = useToast();
  const manage = can("teachers.manage");
  const classes = useAsync(() => api.classes(), []);
  const [classId, setClassId] = useState("");
  const grid = useAsync(() => api.teacherGrid({ class_id: classId || undefined }), [classId]);
  const [saving, setSaving] = useState<string | null>(null);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const staffOptions = useMemo(
    () => [
      { value: "", label: t("alloc.teachers.none") },
      ...(grid.data?.staff ?? []).map((s) => ({ value: s.id, label: s.full_name })),
    ],
    [grid.data, t],
  );
  const setCell = async (section: GridSection, subjectId: string, staffId: string) => {
    const key = `${section.section_id}:${subjectId}`;
    setSaving(key);
    try {
      await api.setTeacherCell({
        section_id: section.section_id,
        subject_id: subjectId,
        staff_id: staffId || null,
      });
      await grid.reload();
    } catch (err) {
      fail(err);
    } finally {
      setSaving(null);
    }
  };
  const setClassTeacher = async (section: GridSection, staffId: string) => {
    setSaving(`${section.section_id}:ct`);
    try {
      await api.updateSection(section.section_id, { class_teacher_id: staffId || null });
      await grid.reload();
    } catch (err) {
      fail(err);
    } finally {
      setSaving(null);
    }
  };
  const data: TeacherGrid | null = grid.data ?? null;
  return (
    <>
      <PageHeader
        title={t("alloc.teachers.title")}
        crumbs={[
          { label: t("nav.dashboard"), to: "/" },
          { label: t("staff.title"), to: "/teachers" },
          { label: t("alloc.teachers.title") },
        ]}
        actions={
          <Select
            label={t("common.class")}
            value={classId}
            onChange={(e) => setClassId(e.target.value)}
            options={[
              { value: "", label: t("alloc.teachers.anyClass") },
              ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
            ]}
            className="w-48"
          />
        }
      />
      <div className="flex flex-col gap-4">
        <p className="text-xs text-muted">{t("alloc.teachers.hint")}</p>
        {grid.loading && !data ? (
          <div className="flex justify-center py-10">
            <Spinner size="lg" />
          </div>
        ) : !data ? (
          <Card>
            <EmptyState title={errorMessage(grid.error, t("common.error.load"))} />
          </Card>
        ) : data.sections.length === 0 ? (
          <Card>
            <EmptyState title={t("academics.sections.empty")} />
          </Card>
        ) : (
          <Card padded={false}>
            <div className="overflow-x-auto">
              <table className="w-full text-sm" aria-label={t("alloc.teachers.title")}>
                <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                  <tr>
                    <th className="sticky start-0 bg-hover px-3 py-2 text-start">
                      {t("alloc.teachers.section")}
                    </th>
                    <th className="px-3 py-2 text-start">{t("alloc.teachers.classTeacher")}</th>
                    {data.subjects.map((s) => (
                      <th key={s.id} className="px-2 py-2 text-start" title={s.name}>
                        {s.code}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {data.sections.map((sec) => {
                    const label = `${sec.class_name} ${sec.section_name}`;
                    return (
                      <tr key={sec.section_id} className="border-t border-border">
                        <td className="sticky start-0 bg-card px-3 py-1.5 font-medium">{label}</td>
                        <td className="px-2 py-1.5">
                          <select
                            aria-label={`${label}: ${t("alloc.teachers.classTeacher")}`}
                            className="h-8 w-40 rounded border border-border bg-card px-2 text-sm"
                            disabled={!manage || saving === `${sec.section_id}:ct`}
                            value={sec.class_teacher_id ?? ""}
                            onChange={(e) => void setClassTeacher(sec, e.target.value)}
                          >
                            {staffOptions.map((o) => (
                              <option key={o.value} value={o.value}>
                                {o.label}
                              </option>
                            ))}
                          </select>
                        </td>
                        {data.subjects.map((s) => {
                          const cell = sec.cells.find((c) => c.subject_id === s.id);
                          if (!sec.subject_ids.includes(s.id)) {
                            return (
                              <td key={s.id} className="px-2 py-1.5 text-center text-xs text-muted">
                                <span title={t("alloc.teachers.notTaught")}>·</span>
                              </td>
                            );
                          }
                          const key = `${sec.section_id}:${s.id}`;
                          return (
                            <td key={s.id} className="px-2 py-1.5">
                              <div className="flex items-center gap-1">
                                <select
                                  aria-label={`${label}: ${s.name}`}
                                  className="h-8 w-36 rounded border border-border bg-card px-2 text-sm"
                                  disabled={!manage || saving === key}
                                  value={cell?.staff_id ?? ""}
                                  onChange={(e) => void setCell(sec, s.id, e.target.value)}
                                >
                                  {staffOptions.map((o) => (
                                    <option key={o.value} value={o.value}>
                                      {o.label}
                                    </option>
                                  ))}
                                </select>
                                {cell && cell.extra > 0 && (
                                  <span className="text-xs text-muted">+{cell.extra}</span>
                                )}
                              </div>
                            </td>
                          );
                        })}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </Card>
        )}
        {data && (
          <Card title={t("alloc.teachers.load")}>
            <p className="mb-2 text-xs text-muted">{t("alloc.teachers.loadHint")}</p>
            <ul
              className="grid gap-1 text-sm sm:grid-cols-2 xl:grid-cols-3"
              aria-label={t("alloc.teachers.load")}
            >
              {data.staff.map((s) => (
                <li
                  key={s.id}
                  className="flex items-center justify-between gap-2 rounded border border-border px-2 py-1"
                >
                  <span>
                    <span className="font-medium">{s.full_name}</span>
                    {s.designation && <span className="text-muted"> · {s.designation}</span>}
                  </span>
                  <span className="text-xs tabular-nums text-muted">
                    {t("alloc.teachers.cells", { cells: s.cells })}
                    {s.class_teacher_of > 0 &&
                      ` · ${t("alloc.teachers.classTeacherOf", { n: s.class_teacher_of })}`}
                  </span>
                </li>
              ))}
            </ul>
          </Card>
        )}
      </div>
    </>
  );
}
