import { useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { ConsentCurrent, ConsentStatus, StudentConsents } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { today } from "./common";

export const CONSENT_TONES: Record<ConsentCurrent, BadgeTone> = {
  given: "success",
  refused: "danger",
  withdrawn: "warning",
  expired: "warning",
  missing: "danger",
};

/** The consents of one student with the history and a form to record a new status. */
export function StudentConsentsTab({ studentId, manage }: { studentId: string; manage: boolean }) {
  const { t } = useI18n();
  const toast = useToast();
  const data = useAsync(() => api.studentConsents(studentId), [studentId]);
  const [typeId, setTypeId] = useState("");
  const [status, setStatus] = useState<ConsentStatus>("given");
  const [guardianId, setGuardianId] = useState("");
  const [date, setDate] = useState(today());
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  if (data.loading && !data.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner size="lg" />
      </div>
    );
  if (data.error || !data.data)
    return (
      <Card>
        <EmptyState title={errorMessage(data.error, t("common.error.load"))} />
      </Card>
    );
  const d: StudentConsents = data.data;
  const chosen = d.types.find((x) => x.id === typeId) ?? d.types[0];
  const effectiveType = typeId || chosen?.id || "";
  const needsGuardian = chosen?.given_by === "guardian" && status === "given";
  const record = async (e: FormEvent) => {
    e.preventDefault();
    if (!effectiveType) return;
    setBusy(true);
    try {
      await api.recordConsent(studentId, {
        consent_type_id: effectiveType,
        status,
        guardian_id: guardianId || null,
        recorded_on: date,
        note,
        academic_year_id: d.academic_year_id,
      });
      toast.push({ kind: "success", title: t("consents.recorded") });
      setNote("");
      await data.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-4">
      <Card title={t("consents.title")} padded={false}>
        {d.types.length === 0 ? (
          <p className="p-4 text-sm text-muted">{t("consents.section.noTypes")}</p>
        ) : (
          <table className="w-full text-sm" aria-label={t("consents.title")}>
            <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
              <tr>
                <th className="px-4 py-2 text-start">{t("consents.types.name")}</th>
                <th className="px-4 py-2 text-start">{t("consents.record.status")}</th>
                <th className="px-4 py-2 text-start">{t("consents.record.guardian")}</th>
                <th className="px-4 py-2 text-start">{t("consents.record.date")}</th>
              </tr>
            </thead>
            <tbody>
              {d.types.map((x) => (
                <tr key={x.id} className="border-t border-border">
                  <td className="px-4 py-2 font-medium">
                    {x.name}
                    {x.renewal === "yearly" && (
                      <span className="ms-2 text-xs text-muted">
                        {t("consents.types.renewal.yearly")}
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-2">
                    {!x.needed && x.current === "missing" ? (
                      <Badge tone="neutral">{t("consents.status.optional")}</Badge>
                    ) : (
                      <Badge tone={CONSENT_TONES[x.current]}>
                        {t(`consents.status.${x.current}`)}
                      </Badge>
                    )}
                  </td>
                  <td className="px-4 py-2 text-muted">{x.latest?.guardian_name ?? "—"}</td>
                  <td className="px-4 py-2 text-muted">{x.latest?.recorded_on ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
      {manage && d.types.length > 0 && (
        <Card title={t("consents.record")}>
          <form onSubmit={record} className="flex flex-wrap items-end gap-2">
            <Select
              label={t("consents.types.name")}
              value={effectiveType}
              onChange={(e) => setTypeId(e.target.value)}
              options={d.types.map((x) => ({ value: x.id, label: x.name }))}
              className="w-48"
            />
            <Select
              label={t("consents.record.status")}
              value={status}
              onChange={(e) => setStatus(e.target.value as ConsentStatus)}
              options={(["given", "refused", "withdrawn"] as const).map((s) => ({
                value: s,
                label: t(`consents.status.${s}`),
              }))}
              className="w-36"
            />
            <Select
              label={t("consents.record.guardian")}
              value={guardianId}
              onChange={(e) => setGuardianId(e.target.value)}
              options={[
                { value: "", label: t("consents.record.guardian.none") },
                ...d.guardians.map((g) => ({
                  value: g.id,
                  label: `${g.full_name} (${g.relationship})`,
                })),
              ]}
              className="w-56"
            />
            <Input
              label={t("consents.record.date")}
              type="date"
              value={date}
              onChange={(e) => setDate(e.target.value)}
              className="w-40"
            />
            <Input
              label={t("consents.record.note")}
              value={note}
              onChange={(e) => setNote(e.target.value)}
              className="w-64"
            />
            <Button type="submit" loading={busy} disabled={needsGuardian && !guardianId}>
              {t("consents.record")}
            </Button>
          </form>
        </Card>
      )}
      <Card title={t("consents.history")} padded={false}>
        {d.history.length === 0 ? (
          <p className="p-4 text-sm text-muted">{t("consents.history.empty")}</p>
        ) : (
          <ul className="divide-y divide-border text-sm" aria-label={t("consents.history")}>
            {d.history.map((h) => (
              <li key={h.id} className="flex flex-wrap items-center gap-2 px-4 py-2">
                <span className="tabular-nums text-muted">{h.recorded_on}</span>
                <span className="font-medium">{h.type_name}</span>
                <Badge tone={CONSENT_TONES[h.status]}>{t(`consents.status.${h.status}`)}</Badge>
                {h.guardian_name && <span className="text-muted">{h.guardian_name}</span>}
                {h.recorded_by_name && (
                  <span className="text-xs text-muted">
                    {t("consents.history.by", { name: h.recorded_by_name })}
                  </span>
                )}
                {h.note && <span className="text-xs text-muted">· {h.note}</span>}
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}
