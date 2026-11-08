import { useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import type { AllocationView, HostelBoard, HostelDetail } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { AllocateDialog, today } from "./common";

/** Occupancy board: every room with its beds; click a free bed to allocate, a taken one to move or end. */
export function BoardTab({ manage }: { manage: boolean }) {
  const { t } = useI18n();
  const toast = useToast();
  const years = useAsync(() => api.academicYears(), []);
  const active = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const [yearId, setYearId] = useState("");
  const year = yearId || active?.id || "";
  const hostels = useAsync(() => api.hostels(), []);
  const [hostelId, setHostelId] = useState("");
  const hostel = hostelId || hostels.data?.[0]?.id || "";
  const board = useAsync<HostelBoard | null>(
    () => (hostel && year ? api.hostelBoard(hostel, year) : Promise.resolve(null)),
    [hostel, year],
  );
  const detail = useAsync<HostelDetail | null>(
    () => (hostel ? api.hostel(hostel) : Promise.resolve(null)),
    [hostel],
  );
  const [allocating, setAllocating] = useState<{ roomId: string; bedNo: number } | null>(null);
  const [moving, setMoving] = useState<AllocationView | null>(null);
  const [busy, setBusy] = useState(false);
  const refresh = () => {
    void board.reload();
    void detail.reload();
    void hostels.reload();
  };
  const end = async (a: AllocationView) => {
    if (!window.confirm(t("hst.allocate.endConfirm", { name: a.student_name }))) return;
    setBusy(true);
    try {
      await api.updateAllocation(a.id, { to_date: today() });
      toast.push({ kind: "success", title: t("hst.allocate.ended") });
      refresh();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const b = board.data;
  return (
    <Card
      actions={
        <div className="flex flex-wrap items-end gap-2">
          <Select
            label={t("tt.year")}
            value={year}
            onChange={(e) => setYearId(e.target.value)}
            options={(years.data ?? []).map((y) => ({ value: y.id, label: y.name }))}
            className="w-36"
          />
          <Select
            label={t("hst.hostel")}
            value={hostel}
            onChange={(e) => setHostelId(e.target.value)}
            options={(hostels.data ?? []).map((h) => ({ value: h.id, label: h.name }))}
            className="w-44"
          />
        </div>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("hst.board.hint")}</p>
      {board.loading && !b ? (
        <Spinner />
      ) : !b ? (
        <EmptyState title={t("hst.hostel.empty")} />
      ) : (
        <div data-testid="board">
          <div className="mb-3 flex flex-wrap items-center gap-2 text-sm">
            <span className="font-semibold">{b.name}</span>
            {b.warden_name && <span className="text-muted">{b.warden_name}</span>}
            <Badge tone={b.occupancy.free === 0 ? "warning" : "success"}>
              {t("hst.board.occupancy", { occupied: b.occupancy.occupied, beds: b.occupancy.beds })}
            </Badge>
          </div>
          {b.by_room.length === 0 ? (
            <EmptyState title={t("hst.room.empty")} />
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {b.by_room.map(({ room, beds }) => (
                <div
                  key={room.id}
                  className={`rounded border border-border p-2 ${room.active ? "" : "opacity-60"}`}
                >
                  <div className="mb-1 flex items-baseline gap-2 text-sm">
                    <span className="font-medium">{room.name}</span>
                    {room.floor && <span className="text-xs text-muted">{room.floor}</span>}
                    <span className="ms-auto text-xs text-muted">
                      {room.occupied}/{room.beds}
                    </span>
                  </div>
                  <ul className="flex flex-col gap-1">
                    {beds.map((bed) => (
                      <li key={bed.bed_no} className="flex items-center gap-2 text-sm">
                        <span className="w-6 text-xs text-muted">{bed.bed_no}</span>
                        {bed.resident ? (
                          <>
                            <span className="font-medium">{bed.resident.student_name}</span>
                            <span className="text-xs text-muted">
                              {bed.resident.class_name}-{bed.resident.section_name}
                            </span>
                            {manage && (
                              <span className="ms-auto flex gap-1">
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  disabled={busy}
                                  onClick={() => setMoving(bed.resident)}
                                >
                                  {t("hst.allocate.moveShort")}
                                </Button>
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  disabled={busy}
                                  onClick={() => void end(bed.resident!)}
                                >
                                  {t("hst.allocate.end")}
                                </Button>
                              </span>
                            )}
                          </>
                        ) : manage && room.active ? (
                          <button
                            type="button"
                            className="text-blue hover:underline"
                            onClick={() => setAllocating({ roomId: room.id, bedNo: bed.bed_no })}
                            aria-label={`${room.name} ${bed.bed_no}`}
                          >
                            {t("hst.board.free")}
                          </button>
                        ) : (
                          <span className="text-muted">{t("hst.board.free")}</span>
                        )}
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
      {detail.data && (allocating || moving) && (
        <AllocateDialog
          hostel={detail.data}
          yearId={year}
          roomId={allocating?.roomId ?? moving?.room_id}
          bedNo={allocating?.bedNo}
          student={moving ?? undefined}
          onClose={() => {
            setAllocating(null);
            setMoving(null);
          }}
          onDone={refresh}
        />
      )}
    </Card>
  );
}
