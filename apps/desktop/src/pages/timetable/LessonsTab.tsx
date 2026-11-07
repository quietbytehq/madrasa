import { useState } from "react";
import { api } from "@/api/endpoints";
import type { LessonBody, LessonView, TimetableSetup } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/useAsync";
import { CheckList, Hint } from "./common";

interface Props {
  setup: TimetableSetup;
  manage: boolean;
  reload: () => Promise<void>;
}

/** Lessons (who teaches what to whom), the load summary and the pre-check findings. */
export function LessonsTab({ setup, manage, reload }: Props) {
  const { t } = useI18n();
  const toast = useToast();
  const [editing, setEditing] = useState<{ id: string | null; body: LessonBody } | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async (f: () => Promise<unknown>, done?: (r: unknown) => string | undefined) => {
    setBusy(true);
    try {
      const r = await f();
      await reload();
      const msg = done?.(r);
      if (msg) toast.push({ kind: "success", title: msg });
      setEditing(null);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const blank = (): LessonBody => ({
    subject_id: setup.subjects[0]?.id ?? "",
    teacher_ids: [],
    section_ids: [],
    room_ids: [],
    periods_per_card: 1,
    cards_per_week: 5,
  });
  const edit = (l: LessonView) =>
    setEditing({
      id: l.id,
      body: {
        subject_id: l.subject_id,
        teacher_ids: l.teacher_ids,
        section_ids: l.section_ids,
        room_ids: l.room_ids,
        periods_per_card: l.periods_per_card,
        cards_per_week: l.cards_per_week,
      },
    });
  const body = editing?.body;
  return (
    <div className="flex flex-col gap-4">
      <CheckCard setup={setup} />
      <Card
        title={t("tt.tab.lessons")}
        actions={
          manage && (
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="secondary"
                loading={busy}
                onClick={() =>
                  void run(
                    () => api.seedLessons(setup.academic_year.id),
                    (r) => t("tt.lessons.seeded", { n: (r as { created: number }).created }),
                  )
                }
              >
                {t("tt.lessons.seed")}
              </Button>
              <Button size="sm" onClick={() => setEditing({ id: null, body: blank() })}>
                {t("tt.lessons.add")}
              </Button>
            </div>
          )
        }
      >
        <Hint>{t("tt.lessons.hint")}</Hint>
        {setup.lessons.length === 0 ? (
          <EmptyState title={t("tt.lessons.empty")} />
        ) : (
          <Table<LessonView>
            rows={setup.lessons}
            rowKey={(l) => l.id}
            filterable
            pageSize={25}
            onRowClick={manage ? edit : undefined}
            columns={[
              { key: "subject", header: t("tt.lesson.subject"), sortable: true },
              {
                key: "sections",
                header: t("tt.lesson.sections"),
                accessor: (l) => l.sections.join(", "),
                render: (l) => l.sections.join(", "),
                sortable: true,
              },
              {
                key: "teachers",
                header: t("tt.lesson.teachers"),
                accessor: (l) => l.teachers.join(", "),
                render: (l) =>
                  l.teachers.length ? (
                    l.teachers.join(", ")
                  ) : (
                    <Badge tone="warning">{t("tt.lesson.noTeacher")}</Badge>
                  ),
              },
              {
                key: "per_week",
                header: t("tt.lesson.perWeek"),
                align: "end",
                accessor: (l) => l.cards_per_week,
                render: (l) =>
                  l.periods_per_card > 1
                    ? `${l.cards_per_week} × ${l.periods_per_card}`
                    : String(l.cards_per_week),
              },
              {
                key: "periods",
                header: t("tt.lesson.periods"),
                align: "end",
                accessor: (l) => l.cards_per_week * l.periods_per_card,
                render: (l) => l.cards_per_week * l.periods_per_card,
                sortable: true,
              },
              {
                key: "rooms",
                header: t("tt.lesson.rooms"),
                render: (l) => l.rooms.join(", ") || "—",
              },
            ]}
          />
        )}
      </Card>
      <LoadCard setup={setup} />
      {editing && body && (
        <Dialog
          open
          onClose={() => setEditing(null)}
          title={editing.id ? t("tt.lesson.edit") : t("tt.lessons.add")}
          size="lg"
          footer={
            <div className="flex w-full items-center justify-between gap-2">
              <div>
                {editing.id && (
                  <Button
                    variant="danger"
                    size="sm"
                    disabled={busy}
                    onClick={() => {
                      if (window.confirm(t("tt.lesson.deleteConfirm")))
                        void run(() => api.deleteLesson(editing.id!));
                    }}
                  >
                    {t("action.delete")}
                  </Button>
                )}
              </div>
              <div className="flex gap-2">
                <Button variant="secondary" onClick={() => setEditing(null)}>
                  {t("action.cancel")}
                </Button>
                <Button
                  loading={busy}
                  disabled={!body.subject_id || body.section_ids.length === 0}
                  onClick={() =>
                    void run(() =>
                      editing.id
                        ? api.updateLesson(editing.id, body)
                        : api.createLesson({ ...body, academic_year_id: setup.academic_year.id }),
                    )
                  }
                >
                  {t("action.save")}
                </Button>
              </div>
            </div>
          }
        >
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-end gap-3">
              <Select
                label={t("tt.lesson.subject")}
                value={body.subject_id}
                onChange={(e) =>
                  setEditing({ ...editing, body: { ...body, subject_id: e.target.value } })
                }
                options={setup.subjects.map((s) => ({ value: s.id, label: s.name }))}
                className="w-56"
              />
              <Select
                label={t("tt.lesson.length")}
                value={String(body.periods_per_card)}
                onChange={(e) =>
                  setEditing({
                    ...editing,
                    body: { ...body, periods_per_card: Number(e.target.value) },
                  })
                }
                options={[1, 2, 3].map((n) => ({
                  value: String(n),
                  label: t(`tt.lesson.length.${n}` as "tt.lesson.length.1"),
                }))}
                className="w-40"
              />
              <Input
                label={t("tt.lesson.perWeek")}
                type="number"
                min={1}
                max={40}
                value={body.cards_per_week}
                onChange={(e) =>
                  setEditing({
                    ...editing,
                    body: { ...body, cards_per_week: Number(e.target.value) || 1 },
                  })
                }
                className="w-28"
              />
            </div>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
              <CheckList
                label={t("tt.lesson.sections")}
                items={setup.sections}
                value={body.section_ids}
                onChange={(ids) => setEditing({ ...editing, body: { ...body, section_ids: ids } })}
                columns={1}
              />
              <CheckList
                label={t("tt.lesson.teachers")}
                items={setup.teachers}
                value={body.teacher_ids}
                onChange={(ids) => setEditing({ ...editing, body: { ...body, teacher_ids: ids } })}
                columns={1}
              />
              <CheckList
                label={t("tt.lesson.rooms")}
                items={setup.rooms.map((r) => ({ id: r.id, name: r.name }))}
                value={body.room_ids}
                onChange={(ids) => setEditing({ ...editing, body: { ...body, room_ids: ids } })}
                columns={1}
              />
            </div>
          </div>
        </Dialog>
      )}
    </div>
  );
}

export function CheckCard({ setup }: { setup: TimetableSetup }) {
  const { t } = useI18n();
  const { check } = setup;
  return (
    <Card
      title={t("tt.check")}
      actions={
        <div className="flex gap-2">
          {check.errors > 0 && (
            <Badge tone="danger">{t("tt.check.errors", { n: check.errors })}</Badge>
          )}
          {check.warnings > 0 && (
            <Badge tone="warning">{t("tt.check.warnings", { n: check.warnings })}</Badge>
          )}
          {check.errors === 0 && check.warnings === 0 && (
            <Badge tone="success">{t("tt.check.clean")}</Badge>
          )}
        </div>
      }
    >
      {check.findings.length === 0 ? (
        <p className="text-sm text-muted">{t("tt.check.clean")}</p>
      ) : (
        <ul className="flex flex-col gap-2 text-sm" data-testid="check-findings">
          {check.findings.map((f, i) => (
            <li key={i} className="flex gap-2">
              <Badge tone={f.level === "error" ? "danger" : "warning"}>{f.title}</Badge>
              <div>
                <div>{f.message}</div>
                {f.fix && (
                  <div className="text-xs text-muted">
                    {t("tt.check.fix")}: {f.fix}
                  </div>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

function LoadCard({ setup }: { setup: TimetableSetup }) {
  const { t } = useI18n();
  const cols = [
    { key: "name", header: "", render: (r: { name: string }) => r.name },
    {
      key: "used",
      header: t("tt.load.used"),
      align: "end" as const,
      render: (r: { used: number; available: number }) => (
        <span className={r.used > r.available ? "font-medium text-danger" : ""}>{r.used}</span>
      ),
    },
    { key: "available", header: t("tt.load.available"), align: "end" as const },
  ];
  return (
    <Card title={t("tt.load")}>
      <Hint>{t("tt.load.hint")}</Hint>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <div>
          <div className="mb-1 text-xs font-medium text-muted">{t("tt.load.sections")}</div>
          <Table rows={setup.load.classes} rowKey={(r) => r.id} columns={cols} pageSize={0} />
        </div>
        <div>
          <div className="mb-1 text-xs font-medium text-muted">{t("tt.load.teachers")}</div>
          <Table rows={setup.load.teachers} rowKey={(r) => r.id} columns={cols} pageSize={0} />
        </div>
      </div>
    </Card>
  );
}
