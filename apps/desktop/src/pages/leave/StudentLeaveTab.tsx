import { useState } from "react";
import { api } from "@/api/endpoints";
import type { StudentLeaveView } from "@/api/types";
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
import { StatusBadge, today } from "./common";

/**
 * Student leave requests: the list for one student (profile tab) or for the sections the
 * caller may see (Leave module), with approval and withdrawal.
 */
export function StudentLeaveTab({
  studentId,
  sectionId,
  showStudent = false,
}: {
  studentId?: string;
  sectionId?: string;
  showStudent?: boolean;
}) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const list = useAsync(
    () => api.studentLeave({ student_id: studentId, section_id: sectionId }),
    [studentId, sectionId],
  );
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const data = list.data;
  const act = async (f: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await f();
      await list.reload();
      toast.push({ kind: "success", title: done });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const canDecide = (r: StudentLeaveView) =>
    !!data &&
    r.status === "pending" &&
    (data.decide_sections === null ||
      (r.section_id !== null && data.decide_sections.includes(r.section_id)));
  return (
    <Card
      title={showStudent ? undefined : t("leave.tab.students")}
      actions={
        data?.can_manage && (
          <Button size="sm" onClick={() => setAdding(true)}>
            {t("leave.students.new")}
          </Button>
        )
      }
    >
      <p className="mb-3 text-xs text-muted">{t("leave.students.hint")}</p>
      {list.loading && !data ? (
        <div className="flex justify-center py-6">
          <Spinner />
        </div>
      ) : !data || data.requests.length === 0 ? (
        <EmptyState title={t("leave.students.empty")} />
      ) : (
        <Table<StudentLeaveView>
          rows={data.requests}
          rowKey={(r) => r.id}
          pageSize={25}
          columns={[
            ...(showStudent
              ? [
                  {
                    key: "student",
                    header: t("leave.students.student"),
                    render: (r: StudentLeaveView) => (
                      <span>
                        <span className="font-medium">{r.full_name}</span>{" "}
                        <span className="text-xs text-muted">{r.student_number}</span>
                      </span>
                    ),
                  },
                  {
                    key: "class",
                    header: t("leave.students.class"),
                    render: (r: StudentLeaveView) => `${r.class_name}-${r.section_name}`,
                  },
                ]
              : []),
            {
              key: "dates",
              header: t("leave.request.from"),
              render: (r) =>
                r.from_date === r.to_date
                  ? formatDate(r.from_date)
                  : `${formatDate(r.from_date)} – ${formatDate(r.to_date)}`,
            },
            { key: "days", header: t("leave.request.days"), align: "end", render: (r) => r.days },
            { key: "reason", header: t("leave.request.reason"), render: (r) => r.reason || "—" },
            {
              key: "guardian",
              header: t("leave.students.guardian"),
              render: (r) => r.guardian_name ?? t("leave.students.noGuardian"),
            },
            {
              key: "status",
              header: t("common.status"),
              render: (r) => (
                <span className="flex flex-wrap items-center gap-1">
                  <StatusBadge status={r.status} />
                  {r.decided_by_name && (
                    <span className="text-xs text-muted">{r.decided_by_name}</span>
                  )}
                  {r.decision_note && <span className="text-xs text-muted">{r.decision_note}</span>}
                </span>
              ),
            },
            {
              key: "actions",
              header: "",
              render: (r) => (
                <div className="flex justify-end gap-1">
                  {canDecide(r) && (
                    <>
                      <Button
                        size="sm"
                        disabled={busy}
                        onClick={() =>
                          void act(
                            () => api.decideStudentLeave(r.id, "approved"),
                            t("leave.request.decided"),
                          )
                        }
                      >
                        {t("leave.request.approve")}
                      </Button>
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy}
                        onClick={() => {
                          const note = window.prompt(t("leave.request.note")) ?? "";
                          void act(
                            () => api.decideStudentLeave(r.id, "rejected", note),
                            t("leave.request.decided"),
                          );
                        }}
                      >
                        {t("leave.request.reject")}
                      </Button>
                    </>
                  )}
                  {data.can_manage && (r.status === "pending" || r.status === "approved") && (
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => {
                        if (window.confirm(t("leave.request.withdrawConfirm")))
                          void act(
                            () => api.withdrawStudentLeave(r.id),
                            t("leave.request.withdrawn"),
                          );
                      }}
                    >
                      {t("leave.request.withdraw")}
                    </Button>
                  )}
                </div>
              ),
            },
          ]}
        />
      )}
      {adding && (
        <NewStudentLeaveDialog
          studentId={studentId}
          busy={busy}
          onClose={() => setAdding(false)}
          onSave={(body) =>
            void act(() => api.createStudentLeave(body), t("leave.request.saved")).then(() =>
              setAdding(false),
            )
          }
        />
      )}
    </Card>
  );
}

function NewStudentLeaveDialog({
  studentId,
  busy,
  onClose,
  onSave,
}: {
  studentId?: string;
  busy: boolean;
  onClose: () => void;
  onSave: (body: {
    student_id: string;
    from_date: string;
    to_date: string;
    reason: string;
    guardian_id: string | null;
  }) => void;
}) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [student, setStudent] = useState(studentId ?? "");
  const [from, setFrom] = useState(today());
  const [to, setTo] = useState(today());
  const [reason, setReason] = useState("");
  const [guardian, setGuardian] = useState("");
  const matches = useAsync(
    () =>
      studentId || query.trim().length < 2
        ? Promise.resolve(null)
        : api.students({ q: query.trim(), page_size: 10 }),
    [query, studentId],
  );
  const profile = useAsync(
    () => (student ? api.student(student) : Promise.resolve(null)),
    [student],
  );
  const guardians = profile.data?.guardians ?? [];
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("leave.students.new")}
      footer={
        <div className="flex gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!student || !from}
            onClick={() =>
              onSave({
                student_id: student,
                from_date: from,
                to_date: to || from,
                reason,
                guardian_id: guardian || null,
              })
            }
          >
            {t("action.save")}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        {!studentId && (
          <div>
            <Input
              label={t("leave.students.student")}
              placeholder={t("leave.students.pickStudent")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            {matches.data && (
              <ul className="mt-1 max-h-32 overflow-y-auto rounded border border-border text-sm">
                {matches.data.items.map((s) => (
                  <li key={s.id}>
                    <button
                      type="button"
                      className={`w-full px-2 py-1 text-start hover:bg-selection ${student === s.id ? "bg-selection" : ""}`}
                      onClick={() => setStudent(s.id)}
                    >
                      {s.full_name} <span className="text-xs text-muted">{s.student_number}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
        <div className="flex gap-3">
          <Input
            label={t("leave.request.from")}
            type="date"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
          />
          <Input
            label={t("leave.request.to")}
            type="date"
            value={to}
            onChange={(e) => setTo(e.target.value)}
          />
        </div>
        <Input
          label={t("leave.request.reason")}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
        <Select
          label={t("leave.students.guardian")}
          value={guardian}
          onChange={(e) => setGuardian(e.target.value)}
          options={[
            { value: "", label: t("leave.students.noGuardian") },
            ...guardians.map((g) => ({ value: g.id, label: g.full_name })),
          ]}
        />
      </div>
    </Dialog>
  );
}
