import { useEffect, useMemo, useState } from "react";
import { api } from "@/api/endpoints";
import type { AssessmentType, SectionDayStatus, SheetStatus } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import { Select } from "@/components/ui/Select";
import type { TFunction } from "@/i18n";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";

const TONES: Record<SheetStatus, BadgeTone> = {
  draft: "warning",
  submitted: "info",
  approved: "success",
};

export function SheetStatusBadge({ status, t }: { status: SheetStatus | null; t: TFunction }) {
  if (!status) return <Badge tone="neutral">{t("exams.status.none")}</Badge>;
  return <Badge tone={TONES[status]}>{t(`exams.status.${status}`)}</Badge>;
}

/** Decimal string → display with up to two places, "—" when null. */
export function num(v: string | number | null | undefined): string {
  if (v === null || v === undefined || v === "") return "—";
  const n = Number(v);
  if (!Number.isFinite(n)) return String(v);
  return Number.isInteger(n) ? String(n) : n.toFixed(2).replace(/\.?0+$/, "");
}

export function pct(v: string | number | null | undefined): string {
  if (v === null || v === undefined || v === "") return "—";
  return `${Number(v).toFixed(2)}%`;
}

/** Active year, its assessments and the caller's sections; shared by the exam tabs. */
export function useExamContext() {
  const years = useAsync(() => api.academicYears(), []);
  const year = useMemo(() => years.data?.find((y) => y.is_active) ?? null, [years.data]);
  const assessments = useAsync(
    () => (year ? api.assessments(year.id) : Promise.resolve([] as AssessmentType[])),
    [year?.id],
  );
  const status = useAsync(() => api.attendanceStatus(), []);
  const sections: SectionDayStatus[] = useMemo(() => status.data?.sections ?? [], [status.data]);
  return {
    year,
    loading: years.loading || assessments.loading || status.loading,
    assessments: assessments.data ?? [],
    reloadAssessments: assessments.reload,
    sections,
  };
}

/** Assessment + section pickers with sensible defaults. */
export function AssessmentSectionPickers({
  assessments,
  sections,
  assessmentId,
  sectionId,
  onChange,
}: {
  assessments: AssessmentType[];
  sections: SectionDayStatus[];
  assessmentId: string;
  sectionId: string;
  onChange: (next: { assessmentId: string; sectionId: string }) => void;
}) {
  const { t } = useI18n();
  const [init, setInit] = useState(false);
  useEffect(() => {
    if (init) return;
    if (!assessmentId && assessments.length > 0) {
      onChange({
        assessmentId: assessments[0]!.id,
        sectionId: sectionId || sections[0]?.section_id || "",
      });
      setInit(sections.length > 0);
    } else if (!sectionId && sections.length > 0) {
      onChange({ assessmentId, sectionId: sections[0]!.section_id });
      setInit(true);
    }
  }, [assessments, sections, assessmentId, sectionId, init, onChange]);
  return (
    <>
      <Select
        label={t("exams.assessment")}
        value={assessmentId}
        onChange={(e) => onChange({ assessmentId: e.target.value, sectionId })}
        options={assessments.map((a) => ({ value: a.id, label: a.name }))}
        className="w-52"
      />
      <Select
        label={t("exams.section")}
        value={sectionId}
        onChange={(e) => onChange({ assessmentId, sectionId: e.target.value })}
        options={sections.map((s) => ({
          value: s.section_id,
          label: `${s.class_name}-${s.section_name}`,
        }))}
        className="w-44"
      />
    </>
  );
}
