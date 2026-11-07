import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { StudentSubjects } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

/** The subjects a student takes this year (M2 stage 6), editable with `students.manage`. */
export function StudentSubjectsTab({ studentId, manage }: { studentId: string; manage: boolean }) {
  const { t } = useI18n();
  const toast = useToast();
  const data = useAsync(() => api.studentSubjects(studentId), [studentId]);
  const [groupId, setGroupId] = useState<string>("");
  const [chosen, setChosen] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const reset = (d: StudentSubjects) => {
    setGroupId(d.group_id ?? "");
    setChosen(
      d.subjects.filter((s) => s.is_elective && s.taken && d.allocated).map((s) => s.subject_id),
    );
  };
  useEffect(() => {
    if (data.data) reset(data.data);
  }, [data.data]);
  if (data.loading && !data.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (data.error || !data.data)
    return (
      <Card>
        <EmptyState title={errorMessage(data.error, t("common.error.load"))} />
      </Card>
    );
  const d = data.data;
  const compulsory = d.subjects.filter((s) => !s.is_elective);
  const electives = d.subjects.filter((s) => s.is_elective);
  const dirty =
    groupId !== (d.group_id ?? "") ||
    chosen.join() !==
      (d.allocated
        ? d.subjects
            .filter((s) => s.is_elective && s.taken)
            .map((s) => s.subject_id)
            .join()
        : "");
  const save = async (clear = false) => {
    setBusy(true);
    try {
      const next = await api.setStudentSubjects(studentId, {
        academic_year_id: d.academic_year_id,
        group_id: clear ? null : groupId || null,
        subject_ids: clear ? [] : chosen,
      });
      toast.push({ kind: "success", title: t("subjects.alloc.saved") });
      await data.reload();
      reset(next);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title={t("students.tabs.subjects")}>
      <div className="flex flex-col gap-3 text-sm">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-muted">{t("common.class")}:</span>
          <span className="font-medium">{d.class_name}</span>
          <Badge tone={d.allocated ? "success" : "neutral"}>
            {d.allocated ? t("subjects.alloc.allocated") : t("subjects.alloc.allSubjects")}
          </Badge>
        </div>
        <div>
          <div className="mb-1 text-xs font-medium text-muted">
            {t("subjects.alloc.compulsory")}
          </div>
          <div className="flex flex-wrap gap-2">
            {compulsory.map((s) => (
              <Badge key={s.subject_id} tone="neutral">
                {s.name}
              </Badge>
            ))}
            {compulsory.length === 0 && <span className="text-muted">—</span>}
          </div>
        </div>
        {electives.length === 0 ? (
          <p className="text-muted">{t("subjects.alloc.noElectives")}</p>
        ) : (
          <>
            <Select
              label={t("subjects.alloc.group")}
              value={groupId}
              disabled={!manage}
              onChange={(e) => {
                const g = d.groups.find((x) => x.id === e.target.value);
                setGroupId(e.target.value);
                if (g) setChosen(g.subject_ids);
              }}
              options={[
                { value: "", label: t("subjects.alloc.none") },
                ...d.groups.map((g) => ({ value: g.id, label: g.name })),
              ]}
              className="w-64"
            />
            <fieldset>
              <legend className="mb-1 text-xs font-medium text-muted">
                {t("subjects.alloc.electives")}
              </legend>
              <div className="flex flex-wrap gap-3">
                {electives.map((s) => (
                  <label key={s.subject_id} className="flex items-center gap-1">
                    <input
                      type="checkbox"
                      className="h-4 w-4 accent-blue"
                      disabled={!manage}
                      checked={manage ? chosen.includes(s.subject_id) : s.taken}
                      onChange={(e) =>
                        setChosen((prev) =>
                          e.target.checked
                            ? [...prev, s.subject_id]
                            : prev.filter((x) => x !== s.subject_id),
                        )
                      }
                    />
                    {s.name}
                  </label>
                ))}
              </div>
            </fieldset>
            {manage && (
              <div className="flex gap-2">
                <Button size="sm" loading={busy} disabled={!dirty} onClick={() => void save()}>
                  {t("subjects.alloc.save")}
                </Button>
                {d.allocated && (
                  <Button size="sm" variant="ghost" loading={busy} onClick={() => void save(true)}>
                    {t("subjects.alloc.clear")}
                  </Button>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </Card>
  );
}
