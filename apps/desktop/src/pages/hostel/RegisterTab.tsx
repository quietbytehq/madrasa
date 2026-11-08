import { useState } from "react";
import { api } from "@/api/endpoints";
import type { AllocationView, RegisterKind, HostelRegisterEntry } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
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

function isOverdue(e: HostelRegisterEntry): boolean {
  return !e.ended_at && !!e.expected_back && new Date(e.expected_back) < new Date();
}

function toLocalInput(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Leave-outs and visitors: who is out, who is expected back, record returns. */
export function RegisterTab({ manage }: { manage: boolean }) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const hostels = useAsync(() => api.hostels(), []);
  const [hostelId, setHostelId] = useState("");
  const [openOnly, setOpenOnly] = useState(true);
  const entries = useAsync(
    () => api.hostelRegister({ hostel_id: hostelId || undefined, open: openOnly || undefined }),
    [hostelId, openOnly],
  );
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const close = async (e: HostelRegisterEntry) => {
    setBusy(true);
    try {
      await api.closeRegister(e.id);
      toast.push({
        kind: "success",
        title: t(e.kind === "visitor" ? "hst.register.visitorLeft" : "hst.register.back"),
      });
      await entries.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const when = (v: string | null) =>
    v
      ? `${formatDate(v)} ${new Date(v).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`
      : "";
  return (
    <Card
      actions={
        <div className="flex flex-wrap items-end gap-2">
          <Select
            label={t("hst.hostel")}
            value={hostelId}
            onChange={(e) => setHostelId(e.target.value)}
            options={[
              { value: "", label: t("hst.allHostels") },
              ...(hostels.data ?? []).map((h) => ({ value: h.id, label: h.name })),
            ]}
            className="w-44"
          />
          <label className="flex items-center gap-2 pb-2 text-sm">
            <input
              type="checkbox"
              checked={openOnly}
              onChange={(e) => setOpenOnly(e.target.checked)}
            />
            {t("hst.register.openOnly")}
          </label>
          {manage && (
            <Button size="sm" onClick={() => setAdding(true)} data-testid="add-entry">
              {t("hst.register.add")}
            </Button>
          )}
        </div>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("hst.register.hint")}</p>
      {entries.loading && !entries.data ? (
        <Spinner />
      ) : (entries.data ?? []).length === 0 ? (
        <EmptyState title={t("hst.register.empty")} />
      ) : (
        <Table<HostelRegisterEntry>
          rows={entries.data ?? []}
          rowKey={(e) => e.id}
          pageSize={50}
          columns={[
            {
              key: "kind",
              header: t("hst.register.kind"),
              render: (e) => (
                <Badge
                  tone={
                    e.kind === "visitor"
                      ? "neutral"
                      : isOverdue(e)
                        ? "danger"
                        : e.ended_at
                          ? "neutral"
                          : "warning"
                  }
                >
                  {t(`hst.register.kind.${e.kind}`)}
                  {isOverdue(e) && ` · ${t("hst.register.overdue")}`}
                </Badge>
              ),
            },
            { key: "student_name", header: t("hst.resident"), sortable: true },
            {
              key: "visitor_name",
              header: t("hst.register.visitor"),
              render: (e) =>
                e.kind === "visitor" ? (
                  <span>
                    {e.visitor_name}{" "}
                    <span className="text-xs text-muted">
                      {e.visitor_relationship}
                      {e.visitor_phone && ` · ${e.visitor_phone}`}
                    </span>
                  </span>
                ) : e.guardian_consent ? (
                  <span className="text-xs text-muted">
                    {t("hst.register.consentGiven")}
                    {e.consent_note && ` · ${e.consent_note}`}
                  </span>
                ) : (
                  ""
                ),
            },
            {
              key: "started_at",
              header: t("hst.register.out"),
              render: (e) => when(e.started_at),
              sortable: true,
            },
            {
              key: "expected_back",
              header: t("hst.register.expected"),
              render: (e) => when(e.expected_back),
            },
            { key: "ended_at", header: t("hst.register.in"), render: (e) => when(e.ended_at) },
            {
              key: "actions",
              header: "",
              render: (e) =>
                manage &&
                !e.ended_at && (
                  <div className="flex justify-end">
                    <Button size="sm" disabled={busy} onClick={() => void close(e)}>
                      {t(e.kind === "visitor" ? "hst.register.left" : "hst.register.returned")}
                    </Button>
                  </div>
                ),
            },
          ]}
        />
      )}
      {adding && (
        <EntryDialog onClose={() => setAdding(false)} onDone={() => void entries.reload()} />
      )}
    </Card>
  );
}

function EntryDialog({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t } = useI18n();
  const toast = useToast();
  const [kind, setKind] = useState<RegisterKind>("leave_out");
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<AllocationView | null>(null);
  const matches = useAsync(
    () =>
      query.trim().length >= 2 ? api.hostelResidents({ q: query.trim() }) : Promise.resolve([]),
    [query],
  );
  const [expected, setExpected] = useState(() =>
    toLocalInput(new Date(Date.now() + 24 * 3600 * 1000)),
  );
  const [consent, setConsent] = useState(false);
  const [consentNote, setConsentNote] = useState("");
  const [visitor, setVisitor] = useState({ name: "", relationship: "", phone: "" });
  const [notes, setNotes] = useState("");
  const [busy, setBusy] = useState(false);
  const save = async () => {
    if (!picked) return;
    setBusy(true);
    try {
      await api.openRegister({
        student_id: picked.student_id,
        kind,
        expected_back: kind === "leave_out" && expected ? new Date(expected).toISOString() : null,
        visitor_name: visitor.name,
        visitor_relationship: visitor.relationship,
        visitor_phone: visitor.phone,
        guardian_consent: consent,
        consent_note: consentNote,
        notes,
      });
      toast.push({ kind: "success", title: t("hst.register.saved") });
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
      title={t("hst.register.add")}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!picked}
            onClick={() => void save()}
            data-testid="save-entry"
          >
            {t("action.save")}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <Select
          label={t("hst.register.kind")}
          value={kind}
          onChange={(e) => setKind(e.target.value as RegisterKind)}
          options={[
            { value: "leave_out", label: t("hst.register.kind.leave_out") },
            { value: "visitor", label: t("hst.register.kind.visitor") },
          ]}
          className="w-48"
        />
        <div>
          <Input
            label={t("hst.resident")}
            placeholder={t("hst.allocate.search")}
            value={picked ? `${picked.student_name} (${picked.student_number})` : query}
            onChange={(e) => {
              setPicked(null);
              setQuery(e.target.value);
            }}
          />
          {!picked && (matches.data ?? []).length > 0 && (
            <ul
              className="mt-1 max-h-40 overflow-auto rounded border border-border text-sm"
              data-testid="matches"
            >
              {(matches.data ?? []).map((a) => (
                <li key={a.id}>
                  <button
                    type="button"
                    className="flex w-full justify-between px-2 py-1 text-start hover:bg-selection"
                    onClick={() => setPicked(a)}
                  >
                    <span>{a.student_name}</span>
                    <span className="text-muted">
                      {a.hostel_name} · {a.room_name}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        {kind === "leave_out" ? (
          <>
            <Input
              label={t("hst.register.expected")}
              type="datetime-local"
              value={expected}
              onChange={(e) => setExpected(e.target.value)}
              className="w-60"
            />
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={consent}
                onChange={(e) => setConsent(e.target.checked)}
              />
              {t("hst.register.consent")}
            </label>
            <Input
              label={t("hst.register.consentNote")}
              value={consentNote}
              onChange={(e) => setConsentNote(e.target.value)}
            />
          </>
        ) : (
          <div className="grid grid-cols-3 gap-3">
            <Input
              label={t("hst.register.visitor")}
              value={visitor.name}
              onChange={(e) => setVisitor({ ...visitor, name: e.target.value })}
            />
            <Input
              label={t("hst.register.relationship")}
              value={visitor.relationship}
              onChange={(e) => setVisitor({ ...visitor, relationship: e.target.value })}
            />
            <Input
              label={t("hst.register.phone")}
              value={visitor.phone}
              onChange={(e) => setVisitor({ ...visitor, phone: e.target.value })}
            />
          </div>
        )}
        <Input label={t("hst.notes")} value={notes} onChange={(e) => setNotes(e.target.value)} />
      </div>
    </Dialog>
  );
}
