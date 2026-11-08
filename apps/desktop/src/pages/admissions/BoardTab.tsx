import { useState } from "react";
import { api } from "@/api/endpoints";
import type { AdmissionsBoard, ApplicationView } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { errorMessage } from "@/lib/useAsync";
import { KIND_TONES } from "./common";

/** The pipeline board: one column per stage with its applications and counts. */
export function BoardTab({
  board,
  manage,
  onOpen,
  reload,
}: {
  board: AdmissionsBoard;
  manage: boolean;
  onOpen: (id: string) => void;
  reload: () => Promise<void>;
}) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);
  const move = async (a: ApplicationView, stageId: string) => {
    if (stageId === a.stage_id) return;
    setBusy(a.id);
    try {
      await api.moveApplication(a.id, stageId);
      await reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  const reached = (stageId: string) =>
    board.funnel.find((f) => f.stage_id === stageId)?.reached ?? 0;
  return (
    <div>
      <p className="mb-3 text-xs text-muted">{t("adm.board.hint")}</p>
      <div className="flex gap-3 overflow-x-auto pb-2" data-testid="board">
        {board.stages.map((stage) => {
          const cards = board.applications.filter((a) => a.stage_id === stage.id);
          return (
            <div
              key={stage.id}
              className="bg-surface flex w-64 shrink-0 flex-col rounded border border-border"
              data-testid={`stage-${stage.code}`}
            >
              <div className="flex items-center justify-between border-b border-border px-2 py-1.5">
                <span className="text-sm font-medium">{stage.name}</span>
                <span className="flex items-center gap-1">
                  <Badge tone={KIND_TONES[stage.kind]}>{cards.length}</Badge>
                  <span className="text-[10px] text-muted">
                    {t("adm.board.reached", { n: reached(stage.id) })}
                  </span>
                </span>
              </div>
              <div className="flex max-h-[60vh] flex-col gap-2 overflow-y-auto p-2">
                {cards.length === 0 && (
                  <p className="px-1 py-3 text-center text-xs text-muted">{t("adm.board.empty")}</p>
                )}
                {cards.map((a) => (
                  <div
                    key={a.id}
                    className={cn(
                      "rounded border border-border bg-card p-2 text-xs shadow-sm",
                      busy === a.id && "opacity-60",
                    )}
                  >
                    <button
                      type="button"
                      className="w-full text-start"
                      onClick={() => onOpen(a.id)}
                      data-testid={`card-${a.application_no}`}
                    >
                      <div className="font-medium text-navy">{a.full_name}</div>
                      <div className="text-muted">
                        {a.application_no}
                        {a.class_name && ` · ${a.class_name}`}
                        {" · "}
                        {formatDate(a.enquiry_date)}
                      </div>
                      {a.student_number && (
                        <div className="text-success">
                          {t("adm.converted")} · {a.student_number}
                        </div>
                      )}
                    </button>
                    {manage && !a.student_id && (
                      <select
                        aria-label={`${a.full_name}: ${t("adm.move")}`}
                        value={a.stage_id}
                        disabled={busy === a.id}
                        onChange={(e) => void move(a, e.target.value)}
                        className="mt-1 h-6 w-full rounded border border-border bg-card px-1 text-[11px]"
                      >
                        {board.stages.map((s) => (
                          <option key={s.id} value={s.id}>
                            {s.name}
                          </option>
                        ))}
                      </select>
                    )}
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
