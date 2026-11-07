import { useState } from "react";
import { api } from "@/api/endpoints";
import type { LeaveRequestBody, LeaveRequestView } from "@/api/types";
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
import { StatusBadge, days, today } from "./common";

/** Staff leave requests: list with filters, new request, approve/reject, withdraw. */
export function RequestsTab({ manage }: { manage: boolean }) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const [status, setStatus] = useState("");
  const [staffId, setStaffId] = useState("");
  const list = useAsync(
    () => api.leaveRequests({ status: status || undefined, staff_id: staffId || undefined }),
    [status, staffId],
  );
  const staff = useAsync(() => (manage ? api.staff() : Promise.resolve([])), [manage]);
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
  const mine = (r: LeaveRequestView) => data?.own_staff_id === r.staff_id;
  return (
    <Card
      actions={
        <Button size="sm" onClick={() => setAdding(true)} data-testid="new-request">
          {manage ? t("leave.requests.newFor") : t("leave.requests.new")}
        </Button>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("leave.requests.hint")}</p>
      <div className="mb-3 flex flex-wrap items-end gap-3">
        <Select
          label={t("leave.filter.status")}
          value={status}
          onChange={(e) => setStatus(e.target.value)}
          options={[
            { value: "", label: t("leave.status.all") },
            ...(["pending", "approved", "rejected", "withdrawn"] as const).map((s) => ({
              value: s,
              label: t(`leave.status.${s}`),
            })),
          ]}
          className="w-40"
        />
        {manage && (
          <Select
            label={t("leave.filter.staff")}
            value={staffId}
            onChange={(e) => setStaffId(e.target.value)}
            options={[
              { value: "", label: t("leave.filter.everyone") },
              ...(staff.data ?? []).map((s) => ({ value: s.id, label: s.full_name })),
            ]}
            className="w-56"
          />
        )}
      </div>
      {list.loading && !data ? (
        <div className="flex justify-center py-6">
          <Spinner />
        </div>
      ) : !data || data.requests.length === 0 ? (
        <EmptyState title={t("leave.requests.empty")} />
      ) : (
        <Table<LeaveRequestView>
          rows={data.requests}
          rowKey={(r) => r.id}
          pageSize={25}
          columns={[
            {
              key: "staff",
              header: t("leave.request.staff"),
              render: (r) => (
                <span>
                  <span className="font-medium">{r.staff_name}</span>{" "}
                  <span className="text-xs text-muted">{r.staff_number}</span>
                </span>
              ),
            },
            { key: "type", header: t("leave.request.type"), render: (r) => r.type_name },
            {
              key: "dates",
              header: t("leave.request.from"),
              render: (r) =>
                `${r.from_date === r.to_date ? formatDate(r.from_date) : `${formatDate(r.from_date)} – ${formatDate(r.to_date)}`}${
                  r.half_day ? ` · ${t(`leave.request.half.${r.half_day}`)}` : ""
                }`,
            },
            {
              key: "days",
              header: t("leave.request.days"),
              align: "end",
              render: (r) => days(r.days),
            },
            { key: "reason", header: t("leave.request.reason"), render: (r) => r.reason || "—" },
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
                  {data.can_decide && r.status === "pending" && !mine(r) && (
                    <>
                      <Button
                        size="sm"
                        disabled={busy}
                        onClick={() =>
                          void act(
                            () => api.decideLeaveRequest(r.id, "approved"),
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
                            () => api.decideLeaveRequest(r.id, "rejected", note),
                            t("leave.request.decided"),
                          );
                        }}
                      >
                        {t("leave.request.reject")}
                      </Button>
                    </>
                  )}
                  {(manage || mine(r)) && (r.status === "pending" || r.status === "approved") && (
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => {
                        if (window.confirm(t("leave.request.withdrawConfirm")))
                          void act(
                            () => api.withdrawLeaveRequest(r.id),
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
        <NewRequestDialog
          manage={manage}
          ownStaffId={data?.own_staff_id ?? null}
          staff={(staff.data ?? []).map((s) => ({ id: s.id, name: s.full_name }))}
          busy={busy}
          onClose={() => setAdding(false)}
          onSave={(body) =>
            void act(() => api.createLeaveRequest(body), t("leave.request.saved")).then(() =>
              setAdding(false),
            )
          }
        />
      )}
    </Card>
  );
}

function NewRequestDialog({
  manage,
  ownStaffId,
  staff,
  busy,
  onClose,
  onSave,
}: {
  manage: boolean;
  ownStaffId: string | null;
  staff: { id: string; name: string }[];
  busy: boolean;
  onClose: () => void;
  onSave: (body: LeaveRequestBody) => void;
}) {
  const { t } = useI18n();
  const types = useAsync(() => api.leaveTypes(), []);
  const [staffId, setStaffId] = useState(ownStaffId ?? "");
  const [typeId, setTypeId] = useState("");
  const [from, setFrom] = useState(today());
  const [to, setTo] = useState(today());
  const [half, setHalf] = useState<"" | "am" | "pm">("");
  const [reason, setReason] = useState("");
  const [attachment, setAttachment] = useState("");
  const type = typeId || types.data?.[0]?.id || "";
  return (
    <Dialog
      open
      onClose={onClose}
      title={manage ? t("leave.requests.newFor") : t("leave.requests.new")}
      footer={
        <div className="flex gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!type || !from || (manage && !staffId && !ownStaffId)}
            onClick={() =>
              onSave({
                staff_id: staffId || undefined,
                leave_type_id: type,
                from_date: from,
                to_date: half ? from : to || from,
                half_day: half,
                reason,
                attachment_note: attachment,
              })
            }
          >
            {t("action.save")}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        {manage && (
          <Select
            label={t("leave.request.staff")}
            value={staffId}
            onChange={(e) => setStaffId(e.target.value)}
            options={[
              ...(ownStaffId ? [{ value: ownStaffId, label: t("leave.request.me") }] : []),
              ...staff
                .filter((s) => s.id !== ownStaffId)
                .map((s) => ({ value: s.id, label: s.name })),
            ]}
            placeholder={t("leave.request.staff")}
          />
        )}
        <Select
          label={t("leave.request.type")}
          value={type}
          onChange={(e) => setTypeId(e.target.value)}
          options={(types.data ?? []).map((x) => ({
            value: x.id,
            label: `${x.name} · ${days(x.entitlement_days)}`,
          }))}
        />
        <div className="flex flex-wrap gap-3">
          <Input
            label={t("leave.request.from")}
            type="date"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
          />
          <Input
            label={t("leave.request.to")}
            type="date"
            value={half ? from : to}
            disabled={!!half}
            onChange={(e) => setTo(e.target.value)}
          />
          <Select
            label={t("leave.request.half")}
            value={half}
            onChange={(e) => setHalf(e.target.value as "" | "am" | "pm")}
            options={[
              { value: "", label: t("leave.request.half.none") },
              { value: "am", label: t("leave.request.half.am") },
              { value: "pm", label: t("leave.request.half.pm") },
            ]}
            className="w-40"
          />
        </div>
        <Input
          label={t("leave.request.reason")}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
        <Input
          label={t("leave.request.attachment")}
          value={attachment}
          onChange={(e) => setAttachment(e.target.value)}
        />
      </div>
    </Dialog>
  );
}
