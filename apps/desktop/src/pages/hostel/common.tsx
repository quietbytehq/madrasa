import { useState } from "react";
import { api } from "@/api/endpoints";
import type { AllocationView, HostelDetail, StudentListRow } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";

export function today(): string {
  return new Date().toISOString().slice(0, 10);
}

/** Put a student in a bed: search the student (or keep the one given), pick room and bed. */
export function AllocateDialog({
  hostel,
  yearId,
  roomId,
  bedNo,
  student,
  onClose,
  onDone,
}: {
  hostel: HostelDetail;
  yearId: string;
  roomId?: string;
  bedNo?: number;
  /** A resident being moved; the search is hidden. */
  student?: AllocationView;
  onClose: () => void;
  onDone: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<StudentListRow | null>(null);
  const matches = useAsync(
    () =>
      query.trim().length >= 2 && !student
        ? api.students({ q: query.trim(), status: "active", page_size: 8 }).then((p) => p.items)
        : Promise.resolve([]),
    [query, student],
  );
  const [room, setRoom] = useState(roomId ?? hostel.room_list[0]?.id ?? "");
  const [bed, setBed] = useState(bedNo ?? 1);
  const [from, setFrom] = useState(today());
  const [amount, setAmount] = useState("");
  const [busy, setBusy] = useState(false);
  const selectedRoom = hostel.room_list.find((r) => r.id === room);
  const studentId = student?.student_id ?? picked?.id ?? "";
  const save = async () => {
    if (!studentId || !room) return;
    setBusy(true);
    try {
      await api.allocateBed({
        student_id: studentId,
        academic_year_id: yearId,
        room_id: room,
        bed_no: bed,
        from_date: from,
        amount: amount === "" ? null : amount,
      });
      toast.push({ kind: "success", title: t("hst.allocate.done") });
      onDone();
      onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={
        student ? t("hst.allocate.move", { name: student.student_name }) : t("hst.allocate.title")
      }
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!studentId || !room}
            onClick={() => void save()}
            data-testid="allocate"
          >
            {t("action.save")}
          </Button>
        </div>
      }
    >
      {!student && (
        <div className="mb-3">
          <Input
            label={t("hst.allocate.student")}
            placeholder={t("hst.allocate.search")}
            value={picked ? `${picked.full_name} (${picked.student_number})` : query}
            onChange={(e) => {
              setPicked(null);
              setQuery(e.target.value);
            }}
            autoFocus
          />
          {!picked && (matches.data ?? []).length > 0 && (
            <ul
              className="mt-1 max-h-40 overflow-auto rounded border border-border text-sm"
              data-testid="matches"
            >
              {(matches.data ?? []).map((s) => (
                <li key={s.id}>
                  <button
                    type="button"
                    className="flex w-full justify-between px-2 py-1 text-start hover:bg-selection"
                    onClick={() => setPicked(s)}
                  >
                    <span>{s.full_name}</span>
                    <span className="text-muted">
                      {s.student_number} · {s.class_name ?? ""}
                      {s.section_name ? `-${s.section_name}` : ""}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      <div className="grid grid-cols-2 gap-3">
        <Select
          label={t("hst.room")}
          value={room}
          onChange={(e) => {
            setRoom(e.target.value);
            setBed(1);
          }}
          options={hostel.room_list
            .filter((r) => r.active)
            .map((r) => ({
              value: r.id,
              label: `${r.floor ? `${r.floor} · ` : ""}${r.name} (${r.occupied}/${r.beds})`,
            }))}
        />
        <Select
          label={t("hst.bed")}
          value={String(bed)}
          onChange={(e) => setBed(Number(e.target.value))}
          options={Array.from({ length: selectedRoom?.beds ?? 0 }, (_, i) => ({
            value: String(i + 1),
            label: String(i + 1),
          }))}
        />
        <Input
          label={t("hst.allocate.from")}
          type="date"
          value={from}
          onChange={(e) => setFrom(e.target.value)}
        />
        <Input
          label={t("hst.allocate.amount")}
          type="number"
          min={0}
          placeholder={t("hst.allocate.defaultFee")}
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
      </div>
    </Dialog>
  );
}
