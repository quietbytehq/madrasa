import { useEffect, useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import type { ConsentBulkLine, ConsentStatus, Section, SectionConsents } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useSession } from "@/app/session";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";
import { CONSENT_TONES } from "./StudentConsentsTab";
import { today } from "./common";

type Draft = { status: ConsentStatus; guardian_id: string };

/** Bulk consent recording per section (M2 stage 8): students × consent types. */
export function ConsentsPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const toast = useToast();
  const manage = can("students.manage");
  const years = useAsync(() => api.academicYears(), []);
  const classes = useAsync(() => api.classes(), []);
  const activeYear = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const sections = useAsync(
    () =>
      activeYear
        ? api.sections({ academic_year_id: activeYear.id })
        : Promise.resolve([] as Section[]),
    [activeYear?.id],
  );
  const [classId, setClassId] = useState("");
  const [sectionId, setSectionId] = useState("");
  const view = useAsync(
    () => (sectionId ? api.sectionConsents(sectionId) : Promise.resolve(null)),
    [sectionId],
  );
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [date, setDate] = useState(today());
  const [busy, setBusy] = useState(false);
  useEffect(() => setDrafts({}), [view.data]);
  const data: SectionConsents | null = view.data ?? null;
  const key = (studentId: string, typeId: string) => `${studentId}:${typeId}`;
  const save = async () => {
    if (!data) return;
    const rows: ConsentBulkLine[] = Object.entries(drafts).map(([k, d]) => {
      const [student_id, consent_type_id] = k.split(":") as [string, string];
      return { student_id, consent_type_id, status: d.status, guardian_id: d.guardian_id || null };
    });
    if (rows.length === 0) return;
    setBusy(true);
    try {
      await api.recordSectionConsents(data.section_id, rows, date);
      toast.push({ kind: "success", title: t("consents.section.saved", { n: rows.length }) });
      await view.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const classSections = (sections.data ?? []).filter((s) => !classId || s.class_id === classId);
  const changed = Object.keys(drafts).length;
  return (
    <>
      <PageHeader
        title={t("consents.title")}
        crumbs={[
          { label: t("nav.dashboard"), to: "/" },
          { label: t("students.title"), to: "/students" },
          { label: t("consents.title") },
        ]}
        actions={
          manage &&
          data && (
            <div className="flex flex-wrap items-end gap-2">
              <Input
                label={t("consents.section.date")}
                type="date"
                value={date}
                onChange={(e) => setDate(e.target.value)}
                className="w-40"
              />
              <Button loading={busy} disabled={changed === 0} onClick={() => void save()}>
                {t("consents.section.save")}
              </Button>
            </div>
          )
        }
      />
      <div className="flex flex-col gap-4">
        <p className="text-xs text-muted">{t("consents.section.hint")}</p>
        <Card>
          <div className="flex flex-wrap items-end gap-3">
            <Select
              label={t("common.class")}
              value={classId}
              onChange={(e) => {
                setClassId(e.target.value);
                setSectionId("");
              }}
              options={[
                { value: "", label: t("subjects.alloc.anyClass") },
                ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
              ]}
              className="w-48"
            />
            <Select
              label={t("common.section")}
              value={sectionId}
              onChange={(e) => setSectionId(e.target.value)}
              options={[
                { value: "", label: t("consents.section.pickSection") },
                ...classSections.map((s) => ({
                  value: s.id,
                  label: `${(classes.data ?? []).find((c) => c.id === s.class_id)?.name ?? ""} ${s.name}`,
                })),
              ]}
              className="w-56"
            />
            {data && (
              <Badge tone={data.missing > 0 ? "warning" : "success"}>
                {t("consents.section.missing", { n: data.missing })}
              </Badge>
            )}
          </div>
        </Card>
        {!sectionId ? (
          <Card>
            <EmptyState title={t("consents.section.pickSection")} />
          </Card>
        ) : view.loading && !data ? (
          <div className="flex justify-center py-10">
            <Spinner size="lg" />
          </div>
        ) : !data ? (
          <Card>
            <EmptyState title={errorMessage(view.error, t("common.error.load"))} />
          </Card>
        ) : data.types.length === 0 ? (
          <Card>
            <EmptyState title={t("consents.section.noTypes")} />
          </Card>
        ) : (
          <Card padded={false}>
            <div className="overflow-x-auto">
              <table className="w-full text-sm" aria-label={t("consents.title")}>
                <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
                  <tr>
                    <th className="w-12 px-3 py-2 text-start">{t("students.rollNo")}</th>
                    <th className="px-3 py-2 text-start">{t("subjects.alloc.student")}</th>
                    <th className="w-48 px-3 py-2 text-start">{t("consents.section.guardian")}</th>
                    {data.types.map((x) => (
                      <th key={x.id} className="px-2 py-2 text-start">
                        {x.name}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {data.students.map((st) => {
                    const guardianKey = `${st.student_id}:guardian`;
                    const guardian = drafts[guardianKey]?.guardian_id ?? st.guardians[0]?.id ?? "";
                    return (
                      <tr key={st.student_id} className="border-t border-border">
                        <td className="px-3 py-1.5 tabular-nums">{st.roll_no ?? "—"}</td>
                        <td className="px-3 py-1.5">
                          <div className="font-medium">{st.full_name}</div>
                          <div className="text-xs text-muted">{st.student_number}</div>
                        </td>
                        <td className="px-3 py-1.5">
                          <select
                            aria-label={`${st.full_name}: ${t("consents.section.guardian")}`}
                            className="h-8 w-full rounded border border-border bg-card px-2 text-sm"
                            disabled={!manage}
                            value={guardian}
                            onChange={(e) =>
                              setDrafts((prev) => {
                                const next = { ...prev };
                                next[guardianKey] = {
                                  status: "given",
                                  guardian_id: e.target.value,
                                };
                                for (const k of Object.keys(next)) {
                                  if (k.startsWith(`${st.student_id}:`) && k !== guardianKey)
                                    next[k] = { ...next[k]!, guardian_id: e.target.value };
                                }
                                return next;
                              })
                            }
                          >
                            <option value="">{t("consents.record.guardian.none")}</option>
                            {st.guardians.map((g) => (
                              <option key={g.id} value={g.id}>
                                {g.full_name} ({g.relationship})
                              </option>
                            ))}
                          </select>
                        </td>
                        {data.types.map((x) => {
                          const cell = st.cells.find((c) => c.consent_type_id === x.id);
                          const k = key(st.student_id, x.id);
                          const draft = drafts[k];
                          const current = cell?.current ?? "missing";
                          return (
                            <td key={x.id} className="px-2 py-1.5">
                              <div className="flex items-center gap-1">
                                {manage ? (
                                  <select
                                    aria-label={`${st.full_name}: ${x.name}`}
                                    className="h-8 rounded border border-border bg-card px-1 text-sm"
                                    value={draft?.status ?? ""}
                                    onChange={(e) =>
                                      setDrafts((prev) => {
                                        const next = { ...prev };
                                        if (!e.target.value) delete next[k];
                                        else
                                          next[k] = {
                                            status: e.target.value as ConsentStatus,
                                            guardian_id: guardian,
                                          };
                                        return next;
                                      })
                                    }
                                  >
                                    <option value="">
                                      {cell && !cell.required && current === "missing"
                                        ? t("consents.status.optional")
                                        : t(`consents.status.${current}`)}
                                    </option>
                                    {(["given", "refused", "withdrawn"] as const).map((s) => (
                                      <option key={s} value={s}>
                                        {t(`consents.status.${s}`)}
                                      </option>
                                    ))}
                                  </select>
                                ) : (
                                  <Badge tone={CONSENT_TONES[current]}>
                                    {t(`consents.status.${current}`)}
                                  </Badge>
                                )}
                                {!draft &&
                                  cell?.required &&
                                  current !== "given" &&
                                  current !== "refused" && (
                                    <span
                                      className="h-2 w-2 rounded-full bg-danger"
                                      title={t(`consents.status.${current}`)}
                                    />
                                  )}
                              </div>
                            </td>
                          );
                        })}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </Card>
        )}
      </div>
    </>
  );
}
