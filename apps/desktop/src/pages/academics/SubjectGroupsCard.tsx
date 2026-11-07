import { useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { SchoolClass, Subject, SubjectGroup } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";

/** Subject groups per class (M2 stage 6): a named set of electives a student picks at once. */
export function SubjectGroupsCard({
  classes,
  subjects,
  manage,
  onError,
}: {
  classes: SchoolClass[];
  subjects: Subject[];
  manage: boolean;
  onError: (err: unknown) => void;
}) {
  const { t } = useI18n();
  const [classId, setClassId] = useState("");
  const selected = classId || classes[0]?.id || "";
  const electives = useAsync(
    () => (selected ? api.classSubjects(selected) : Promise.resolve(null)),
    [selected],
  );
  const groups = useAsync(
    () => (selected ? api.subjectGroups(selected) : Promise.resolve([] as SubjectGroup[])),
    [selected],
  );
  const [editing, setEditing] = useState<{ id: string | null; name: string; ids: string[] } | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const electiveIds = electives.data?.elective_ids ?? [];
  const electiveSubjects = subjects.filter((s) => electiveIds.includes(s.id));
  const nameOf = (id: string) => subjects.find((s) => s.id === id)?.name ?? "?";

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!editing || !editing.name.trim()) return;
    setBusy(true);
    try {
      const body = {
        name: editing.name.trim(),
        position: editing.id
          ? (groups.data?.find((g) => g.id === editing.id)?.position ?? 0)
          : (groups.data?.length ?? 0),
        subject_ids: editing.ids,
      };
      if (editing.id) await api.updateSubjectGroup(editing.id, body);
      else await api.createSubjectGroup(selected, body);
      setEditing(null);
      await groups.reload();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  const remove = async (id: string) => {
    setBusy(true);
    try {
      await api.deleteSubjectGroup(id);
      await groups.reload();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title={t("academics.groups")} padded={false}>
      <p className="px-4 pt-3 text-xs text-muted">{t("academics.groups.hint")}</p>
      <div className="flex flex-wrap items-end gap-2 p-4">
        <Select
          label={t("common.class")}
          value={selected}
          onChange={(e) => {
            setClassId(e.target.value);
            setEditing(null);
          }}
          options={classes.map((c) => ({ value: c.id, label: c.name }))}
          className="w-48"
        />
        {manage && selected && electiveSubjects.length > 0 && !editing && (
          <Button variant="secondary" onClick={() => setEditing({ id: null, name: "", ids: [] })}>
            {t("academics.groups.new")}
          </Button>
        )}
      </div>
      {groups.loading && !groups.data ? (
        <div className="flex justify-center p-4">
          <Spinner />
        </div>
      ) : electiveSubjects.length === 0 ? (
        <p className="px-4 pb-4 text-sm text-muted">{t("academics.groups.noElectives")}</p>
      ) : (
        <table className="w-full text-sm" aria-label={t("academics.groups")}>
          <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-4 py-2 text-start">{t("academics.groups.name")}</th>
              <th className="px-4 py-2 text-start">{t("academics.groups.subjects")}</th>
              <th className="w-32 px-4 py-2" />
            </tr>
          </thead>
          <tbody>
            {(groups.data ?? []).length === 0 && (
              <tr>
                <td colSpan={3} className="px-4 py-3 text-muted">
                  {t("academics.groups.empty")}
                </td>
              </tr>
            )}
            {(groups.data ?? []).map((g) => (
              <tr key={g.id} className="border-t border-border">
                <td className="px-4 py-2 font-medium">{g.name}</td>
                <td className="px-4 py-2 text-muted">{g.subject_ids.map(nameOf).join(", ")}</td>
                <td className="px-4 py-2 text-end">
                  {manage && (
                    <div className="flex justify-end gap-1">
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => setEditing({ id: g.id, name: g.name, ids: g.subject_ids })}
                      >
                        {t("action.edit")}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        loading={busy}
                        onClick={() => void remove(g.id)}
                      >
                        {t("action.delete")}
                      </Button>
                    </div>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {editing && (
        <form
          onSubmit={save}
          className="flex flex-col gap-2 border-t border-border p-4"
          data-testid="group-editor"
        >
          <Input
            label={t("academics.groups.name")}
            value={editing.name}
            onChange={(e) => setEditing({ ...editing, name: e.target.value })}
            className="w-64"
            required
          />
          <fieldset>
            <legend className="mb-1 text-xs font-medium text-text">
              {t("academics.groups.subjects")}
            </legend>
            <div className="flex flex-wrap gap-3">
              {electiveSubjects.map((s) => (
                <label key={s.id} className="flex items-center gap-1 text-sm">
                  <input
                    type="checkbox"
                    className="h-4 w-4 accent-blue"
                    checked={editing.ids.includes(s.id)}
                    onChange={(e) =>
                      setEditing({
                        ...editing,
                        ids: e.target.checked
                          ? [...editing.ids, s.id]
                          : editing.ids.filter((x) => x !== s.id),
                      })
                    }
                  />
                  {s.name}
                </label>
              ))}
            </div>
          </fieldset>
          <div className="flex gap-2">
            <Button type="submit" size="sm" loading={busy} disabled={!editing.name.trim()}>
              {t("action.save")}
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setEditing(null)}>
              {t("action.cancel")}
            </Button>
          </div>
        </form>
      )}
    </Card>
  );
}
