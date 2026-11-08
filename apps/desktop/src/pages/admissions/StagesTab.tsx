import { useEffect, useState } from "react";
import { api } from "@/api/endpoints";
import type { AdmissionStage, StageBody, StageKind } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { KindBadge } from "./common";

const KINDS: StageKind[] = ["open", "admitted", "rejected", "withdrawn"];

/** Stages of the pipeline and the admission fee head (admissions.manage). */
export function StagesTab({ onChanged }: { onChanged: () => Promise<void> }) {
  const { t } = useI18n();
  const toast = useToast();
  const stages = useAsync(() => api.admissionStages(), []);
  const settings = useAsync(() => api.admissionSettings(), []);
  const heads = useAsync(() => api.feeHeads(), []);
  const [editing, setEditing] = useState<{ id: string | null; body: StageBody } | null>(null);
  const [feeHead, setFeeHead] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => setFeeHead(settings.data?.fee_head_id ?? ""), [settings.data]);
  const run = async (f: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    try {
      await f();
      await stages.reload();
      await onChanged();
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
        title={t("adm.tab.stages")}
        actions={
          <Button
            size="sm"
            onClick={() =>
              setEditing({
                id: null,
                body: { name: "", kind: "open", position: (stages.data?.length ?? 0) + 1 },
              })
            }
          >
            {t("adm.stages.add")}
          </Button>
        }
      >
        <p className="mb-3 text-xs text-muted">{t("adm.stages.hint")}</p>
        <Table<AdmissionStage>
          rows={stages.data ?? []}
          rowKey={(s) => s.id}
          pageSize={0}
          onRowClick={(s) =>
            setEditing({
              id: s.id,
              body: { name: s.name, code: s.code, kind: s.kind, position: s.position },
            })
          }
          columns={[
            { key: "position", header: t("adm.stage.position"), align: "end" },
            { key: "name", header: t("adm.stage.name") },
            { key: "code", header: t("adm.field.no"), render: (s) => s.code },
            {
              key: "kind",
              header: t("adm.stage.kind"),
              render: (s) => <KindBadge kind={s.kind} />,
            },
          ]}
        />
      </Card>
      <Card title={t("adm.settings.feeHead")}>
        <div className="flex flex-wrap items-end gap-3">
          <Select
            label={t("adm.settings.feeHead")}
            value={feeHead}
            onChange={(e) => setFeeHead(e.target.value)}
            options={[
              { value: "", label: t("adm.settings.noFeeHead") },
              ...(heads.data ?? []).map((h) => ({ value: h.id, label: h.name })),
            ]}
            className="w-64"
          />
          <Button
            size="sm"
            loading={busy}
            onClick={() =>
              void run(
                () => api.updateAdmissionSettings(feeHead || null).then((s) => settings.setData(s)),
                t("adm.settings.saved"),
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
          title={editing.id ? t("adm.stage.edit") : t("adm.stages.add")}
          footer={
            <div className="flex w-full items-center justify-between gap-2">
              <div>
                {editing.id && (
                  <Button
                    variant="danger"
                    size="sm"
                    disabled={busy}
                    onClick={() => {
                      if (window.confirm(t("adm.stage.deleteConfirm")))
                        void run(() => api.deleteAdmissionStage(editing.id!));
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
                        ? api.updateAdmissionStage(editing.id, body)
                        : api.createAdmissionStage(body),
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
              label={t("adm.stage.name")}
              value={body.name}
              onChange={(e) => setEditing({ ...editing, body: { ...body, name: e.target.value } })}
            />
            <Select
              label={t("adm.stage.kind")}
              value={body.kind}
              onChange={(e) =>
                setEditing({ ...editing, body: { ...body, kind: e.target.value as StageKind } })
              }
              options={KINDS.map((k) => ({ value: k, label: t(`adm.kind.${k}`) }))}
            />
            <Input
              label={t("adm.stage.position")}
              type="number"
              value={body.position ?? 0}
              onChange={(e) =>
                setEditing({ ...editing, body: { ...body, position: Number(e.target.value) || 0 } })
              }
            />
          </div>
        </Dialog>
      )}
    </div>
  );
}
