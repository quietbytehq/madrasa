import { useState } from "react";
import { api } from "@/api/endpoints";
import type { AdmissionsBoard, ApplicationView } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { StageBadge } from "./common";

/** Applications as a table with filters, ticks and a bulk move. */
export function ListTab({
  board,
  manage,
  query,
  classId,
  onQuery,
  onClass,
  onOpen,
  reload,
}: {
  board: AdmissionsBoard;
  manage: boolean;
  query: string;
  classId: string;
  onQuery: (q: string) => void;
  onClass: (c: string) => void;
  onOpen: (id: string) => void;
  reload: () => Promise<void>;
}) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const classes = useAsync(() => api.classes(), []);
  const [stageId, setStageId] = useState("");
  const [ticked, setTicked] = useState<string[]>([]);
  const [target, setTarget] = useState("");
  const [busy, setBusy] = useState(false);
  const rows = board.applications.filter((a) => !stageId || a.stage_id === stageId);
  const bulk = async () => {
    if (!target || ticked.length === 0) return;
    setBusy(true);
    try {
      const r = await api.moveApplications(ticked, target);
      toast.push({ kind: "success", title: t("adm.move.done", { n: r.moved }) });
      if (r.refused.length > 0) {
        toast.push({
          kind: "warning",
          title: t("adm.move.refused", { n: r.refused.length, reason: r.refused[0]?.[1] ?? "" }),
        });
      }
      setTicked([]);
      await reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card>
      <p className="mb-3 text-xs text-muted">{t("adm.list.hint")}</p>
      <div className="mb-3 flex flex-wrap items-end gap-3">
        <Input
          label={t("action.search")}
          placeholder={t("adm.filter.search")}
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          className="w-56"
        />
        <Select
          label={t("adm.filter.stage")}
          value={stageId}
          onChange={(e) => setStageId(e.target.value)}
          options={[
            { value: "", label: t("adm.filter.anyStage") },
            ...board.stages.map((s) => ({ value: s.id, label: s.name })),
          ]}
          className="w-44"
        />
        <Select
          label={t("adm.filter.class")}
          value={classId}
          onChange={(e) => onClass(e.target.value)}
          options={[
            { value: "", label: t("adm.filter.anyClass") },
            ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
          ]}
          className="w-40"
        />
        {manage && ticked.length > 0 && (
          <>
            <Select
              label={t("adm.move")}
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              options={[
                { value: "", label: "—" },
                ...board.stages.map((s) => ({ value: s.id, label: s.name })),
              ]}
              className="w-44"
            />
            <Button size="sm" loading={busy} disabled={!target} onClick={() => void bulk()}>
              {t("adm.move.selected", { n: ticked.length })}
            </Button>
          </>
        )}
      </div>
      {rows.length === 0 ? (
        <EmptyState title={t("adm.list.empty")} />
      ) : (
        <Table<ApplicationView>
          rows={rows}
          rowKey={(a) => a.id}
          pageSize={25}
          onRowClick={(a) => onOpen(a.id)}
          columns={[
            ...(manage
              ? [
                  {
                    key: "tick",
                    header: "",
                    width: "2rem",
                    render: (a: ApplicationView) => (
                      <input
                        type="checkbox"
                        aria-label={`${t("adm.move")} ${a.full_name}`}
                        checked={ticked.includes(a.id)}
                        onClick={(e) => e.stopPropagation()}
                        onChange={(e) =>
                          setTicked((all) =>
                            e.target.checked ? [...all, a.id] : all.filter((x) => x !== a.id),
                          )
                        }
                      />
                    ),
                  },
                ]
              : []),
            { key: "application_no", header: t("adm.field.no"), sortable: true },
            { key: "full_name", header: t("adm.field.name"), sortable: true },
            {
              key: "class_name",
              header: t("adm.field.class"),
              render: (a) => a.class_name ?? "—",
              sortable: true,
            },
            {
              key: "enquiry_date",
              header: t("adm.field.enquiryDate"),
              render: (a) => formatDate(a.enquiry_date),
              sortable: true,
            },
            {
              key: "stage",
              header: t("adm.field.stage"),
              accessor: (a) => a.stage_name,
              render: (a) => <StageBadge name={a.stage_name} kind={a.stage_kind} />,
            },
            {
              key: "phone",
              header: t("adm.field.phone"),
              render: (a) => a.guardian_phone ?? a.phone ?? "—",
            },
            {
              key: "student",
              header: t("adm.field.student"),
              render: (a) => a.student_number ?? "",
            },
          ]}
        />
      )}
    </Card>
  );
}
