import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { BalancePreview, ClassSections, RollNumbersMode } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";

/** Bulk student moves between the sections of a class, with a balancing proposal (M2 stage 7). */
export function SectionMovesPage() {
  const { t } = useI18n();
  const toast = useToast();
  const classes = useAsync(() => api.classes(), []);
  const [classId, setClassId] = useState("");
  const view = useAsync(
    () => (classId ? api.classSections(classId) : Promise.resolve(null)),
    [classId],
  );
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const [target, setTarget] = useState("");
  const [rollMode, setRollMode] = useState<RollNumbersMode>("reassign");
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<BalancePreview | null>(null);
  useEffect(() => {
    setTicked(new Set());
    setTarget("");
  }, [view.data]);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const data: ClassSections | null = view.data ?? null;
  const toggle = (id: string) =>
    setTicked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const applyMoves = async (moves: Array<{ student_id: string; to_section_id: string }>) => {
    if (!data || moves.length === 0) return;
    setBusy(true);
    try {
      const res = await api.moveStudents({
        class_id: data.class_id,
        academic_year_id: data.academic_year_id,
        moves,
        roll_numbers: rollMode,
      });
      toast.push({ kind: "success", title: t("alloc.moves.moved", { n: res.moved }) });
      setPreview(null);
      await view.reload();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const balance = async () => {
    if (!data) return;
    setBusy(true);
    try {
      setPreview(
        await api.balanceSections({
          class_id: data.class_id,
          academic_year_id: data.academic_year_id,
          exclude_student_ids: [...ticked],
        }),
      );
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const movable = [...ticked].filter((id) =>
    data?.sections.some(
      (s) => s.section_id !== target && s.students.some((st) => st.student_id === id),
    ),
  );
  return (
    <>
      <PageHeader
        title={t("alloc.moves.title")}
        crumbs={[
          { label: t("nav.dashboard"), to: "/" },
          { label: t("students.title"), to: "/students" },
          { label: t("alloc.moves.title") },
        ]}
        actions={
          <div className="flex flex-wrap items-end gap-2">
            <Select
              label={t("common.class")}
              value={classId}
              onChange={(e) => setClassId(e.target.value)}
              options={[
                { value: "", label: t("alloc.moves.pickClass") },
                ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
              ]}
              className="w-44"
            />
            {data && data.sections.length > 1 && (
              <Button variant="secondary" loading={busy} onClick={() => void balance()}>
                {t("alloc.moves.balance")}
              </Button>
            )}
          </div>
        }
      />
      <div className="flex flex-col gap-4">
        <p className="text-xs text-muted">{t("alloc.moves.hint")}</p>
        {!classId ? (
          <Card>
            <EmptyState title={t("alloc.moves.pickClass")} />
          </Card>
        ) : view.loading && !data ? (
          <div className="flex justify-center py-10">
            <Spinner size="lg" />
          </div>
        ) : !data ? (
          <Card>
            <EmptyState title={errorMessage(view.error, t("common.error.load"))} />
          </Card>
        ) : (
          <>
            {data.sections.length < 2 && (
              <p className="text-sm text-muted">{t("alloc.moves.oneSection")}</p>
            )}
            <Card>
              <div className="flex flex-wrap items-end gap-3">
                <Select
                  label={t("alloc.moves.moveTo")}
                  value={target}
                  onChange={(e) => setTarget(e.target.value)}
                  options={[
                    { value: "", label: "—" },
                    ...data.sections.map((s) => ({ value: s.section_id, label: s.name })),
                  ]}
                  className="w-40"
                />
                <Select
                  label={t("alloc.moves.rollNumbers")}
                  value={rollMode}
                  onChange={(e) => setRollMode(e.target.value as RollNumbersMode)}
                  options={[
                    { value: "reassign", label: t("alloc.moves.rollReassign") },
                    { value: "keep", label: t("alloc.moves.rollKeep") },
                  ]}
                  className="w-80"
                />
                <Button
                  loading={busy}
                  disabled={!target || movable.length === 0}
                  onClick={() =>
                    void applyMoves(
                      movable.map((id) => ({ student_id: id, to_section_id: target })),
                    )
                  }
                >
                  {t("alloc.moves.move", { n: movable.length })}
                </Button>
              </div>
            </Card>
            <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
              {data.sections.map((s) => (
                <Card
                  key={s.section_id}
                  title={`${data.class_name} ${s.name}`}
                  padded={false}
                  actions={
                    <span className="text-xs text-muted">
                      {t("alloc.moves.size", { size: s.size })} ·{" "}
                      {t("alloc.moves.genders", { male: s.male, female: s.female })}
                      {s.capacity !== null &&
                        ` · ${t("alloc.moves.capacity", { capacity: s.capacity })}`}
                    </span>
                  }
                >
                  <table className="w-full text-sm" aria-label={`${data.class_name} ${s.name}`}>
                    <tbody>
                      {s.students.map((st) => (
                        <tr key={st.student_id} className="border-t border-border">
                          <td className="w-8 px-3 py-1">
                            <input
                              type="checkbox"
                              className="h-4 w-4 accent-blue"
                              aria-label={st.full_name}
                              checked={ticked.has(st.student_id)}
                              onChange={() => toggle(st.student_id)}
                            />
                          </td>
                          <td className="w-10 px-2 py-1 tabular-nums text-muted">
                            {st.roll_no ?? "—"}
                          </td>
                          <td className="px-2 py-1">
                            {st.full_name}
                            <span className="ms-2 text-xs text-muted">{st.student_number}</span>
                          </td>
                          <td className="px-2 py-1 text-end">
                            <Badge tone="neutral">{t(`gender.${st.gender}`)}</Badge>
                          </td>
                        </tr>
                      ))}
                      {s.students.length === 0 && (
                        <tr>
                          <td className="px-3 py-2 text-muted" colSpan={4}>
                            —
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                </Card>
              ))}
            </div>
          </>
        )}
      </div>
      <Dialog
        open={preview !== null}
        onClose={() => setPreview(null)}
        title={t("alloc.moves.balanceTitle")}
      >
        {preview && (
          <div className="flex flex-col gap-3 text-sm" data-testid="balance-preview">
            <p className="text-xs text-muted">{t("alloc.moves.balanceHint")}</p>
            {preview.moves.length === 0 ? (
              <p>{t("alloc.moves.balanceNone")}</p>
            ) : (
              <table className="w-full text-sm">
                <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                  <tr>
                    <th className="px-2 py-1 text-start">{t("alloc.moves.title")}</th>
                    <th className="px-2 py-1 text-start">{t("alloc.moves.from")}</th>
                    <th className="px-2 py-1 text-start">{t("alloc.moves.to")}</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.moves.map((m) => (
                    <tr key={m.student_id} className="border-t border-border">
                      <td className="px-2 py-1">
                        {m.full_name}
                        <span className="ms-2 text-xs text-muted">{m.student_number}</span>
                      </td>
                      <td className="px-2 py-1">{m.from_name}</td>
                      <td className="px-2 py-1">{m.to_name}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            <div>
              <div className="mb-1 text-xs font-medium text-muted">{t("alloc.moves.sizes")}</div>
              <div className="flex flex-wrap gap-2">
                {preview.sizes.map((s) => (
                  <Badge key={s.section_id} tone="neutral">
                    {s.name}: {s.before} → {s.after}
                  </Badge>
                ))}
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setPreview(null)}>
                {t("action.cancel")}
              </Button>
              <Button
                loading={busy}
                disabled={preview.moves.length === 0}
                onClick={() =>
                  void applyMoves(
                    preview.moves.map((m) => ({
                      student_id: m.student_id,
                      to_section_id: m.to_section_id,
                    })),
                  )
                }
              >
                {t("alloc.moves.apply")}
              </Button>
            </div>
          </div>
        )}
      </Dialog>
    </>
  );
}
