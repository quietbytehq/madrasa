import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { LeaveType, LeaveTypeBody } from "@/api/types";
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
import { errorMessage, useAsync } from "@/lib/useAsync";
import { days } from "./common";

/** Leave types and the approver setting (leave.manage). */
export function TypesTab() {
  const { t } = useI18n();
  const toast = useToast();
  const types = useAsync(() => api.leaveTypes(), []);
  const settings = useAsync(() => api.leaveSettings(), []);
  const staff = useAsync(() => api.staff(), []);
  const [editing, setEditing] = useState<{ id: string | null; body: LeaveTypeBody } | null>(null);
  const [approver, setApprover] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => setApprover(settings.data?.approver_staff_id ?? ""), [settings.data]);
  const run = async (f: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    try {
      await f();
      await types.reload();
      if (done) toast.push({ kind: "success", title: done });
      setEditing(null);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const body = editing?.body;
  return (
    <div className="flex flex-col gap-4">
      <Card
        title={t("leave.tab.types")}
        actions={
          <Button
            size="sm"
            onClick={() =>
              setEditing({
                id: null,
                body: {
                  name: "",
                  code: "",
                  entitlement_days: 10,
                  carry_over_max: 0,
                  needs_approval: true,
                },
              })
            }
          >
            {t("leave.types.add")}
          </Button>
        }
      >
        <p className="mb-3 text-xs text-muted">{t("leave.types.hint")}</p>
        {(types.data ?? []).length === 0 ? (
          <EmptyState title={t("leave.types.empty")} />
        ) : (
          <Table<LeaveType>
            rows={types.data ?? []}
            rowKey={(x) => x.id}
            pageSize={0}
            onRowClick={(x) =>
              setEditing({
                id: x.id,
                body: {
                  name: x.name,
                  code: x.code,
                  entitlement_days: Number(x.entitlement_days),
                  carry_over_max: Number(x.carry_over_max),
                  needs_approval: x.needs_approval,
                  position: x.position,
                },
              })
            }
            columns={[
              { key: "name", header: t("leave.type.name"), render: (x) => x.name },
              { key: "code", header: t("leave.type.code"), render: (x) => x.code || "—" },
              {
                key: "entitlement",
                header: t("leave.type.entitlement"),
                align: "end",
                render: (x) => days(x.entitlement_days),
              },
              {
                key: "carry",
                header: t("leave.type.carry"),
                align: "end",
                render: (x) => days(x.carry_over_max),
              },
              {
                key: "approval",
                header: t("leave.type.approval"),
                render: (x) => (
                  <Badge tone={x.needs_approval ? "info" : "neutral"}>
                    {x.needs_approval ? "✓" : "—"}
                  </Badge>
                ),
              },
            ]}
          />
        )}
      </Card>
      <Card title={t("leave.settings.approver")}>
        <div className="flex flex-wrap items-end gap-3">
          <Select
            label={t("leave.settings.approver")}
            value={approver}
            onChange={(e) => setApprover(e.target.value)}
            options={[
              { value: "", label: t("leave.settings.none") },
              ...(staff.data ?? []).map((s) => ({ value: s.id, label: s.full_name })),
            ]}
            className="w-64"
          />
          <Button
            size="sm"
            loading={busy}
            onClick={() =>
              void run(
                () => api.updateLeaveSettings(approver || null).then((s) => settings.setData(s)),
                t("leave.settings.saved"),
              )
            }
          >
            {t("action.save")}
          </Button>
        </div>
      </Card>
      {editing && body && (
        <Dialog
          open
          onClose={() => setEditing(null)}
          title={editing.id ? t("leave.type.edit") : t("leave.types.add")}
          footer={
            <div className="flex w-full items-center justify-between gap-2">
              <div>
                {editing.id && (
                  <Button
                    variant="danger"
                    size="sm"
                    disabled={busy}
                    onClick={() => {
                      if (window.confirm(t("leave.type.deleteConfirm")))
                        void run(() => api.deleteLeaveType(editing.id!));
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
                  disabled={!body.name.trim()}
                  onClick={() =>
                    void run(() =>
                      editing.id
                        ? api.updateLeaveType(editing.id, body)
                        : api.createLeaveType(body),
                    )
                  }
                >
                  {t("action.save")}
                </Button>
              </div>
            </div>
          }
        >
          <div className="grid grid-cols-2 gap-3">
            <Input
              label={t("leave.type.name")}
              value={body.name}
              onChange={(e) => setEditing({ ...editing, body: { ...body, name: e.target.value } })}
            />
            <Input
              label={t("leave.type.code")}
              value={body.code ?? ""}
              onChange={(e) => setEditing({ ...editing, body: { ...body, code: e.target.value } })}
            />
            <Input
              label={t("leave.type.entitlement")}
              type="number"
              min={0}
              step="0.5"
              value={body.entitlement_days}
              onChange={(e) =>
                setEditing({
                  ...editing,
                  body: { ...body, entitlement_days: Number(e.target.value) || 0 },
                })
              }
            />
            <Input
              label={t("leave.type.carry")}
              type="number"
              min={0}
              step="0.5"
              value={body.carry_over_max ?? 0}
              onChange={(e) =>
                setEditing({
                  ...editing,
                  body: { ...body, carry_over_max: Number(e.target.value) || 0 },
                })
              }
            />
            <label className="col-span-2 flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={body.needs_approval}
                onChange={(e) =>
                  setEditing({ ...editing, body: { ...body, needs_approval: e.target.checked } })
                }
              />
              {t("leave.type.approval")}
            </label>
          </div>
        </Dialog>
      )}
    </div>
  );
}
