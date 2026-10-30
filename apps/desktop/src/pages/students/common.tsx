import type { Gender, Relationship, StudentStatus } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import type { BadgeTone } from "@/components/ui/Badge";
import type { TFunction } from "@/i18n";

export const GENDERS: Gender[] = ["male", "female", "other"];
export const STATUSES: StudentStatus[] = ["active", "left", "graduated"];
export const RELATIONSHIPS: Relationship[] = ["father", "mother", "guardian", "other"];

export function genderLabel(t: TFunction, g: Gender) {
  return t(`gender.${g}`);
}

export function statusLabel(t: TFunction, s: StudentStatus) {
  return t(`students.status.${s}`);
}

export function relationshipLabel(t: TFunction, r: Relationship) {
  return t(`guardians.relationship.${r}`);
}

const tones: Record<StudentStatus, BadgeTone> = {
  active: "success",
  left: "neutral",
  graduated: "lavender",
};

export function StatusBadge({ status, t }: { status: StudentStatus; t: TFunction }) {
  return <Badge tone={tones[status]}>{statusLabel(t, status)}</Badge>;
}

export function classLabel(
  className: string | null | undefined,
  sectionName: string | null | undefined,
) {
  if (!className) return "—";
  return sectionName ? `${className}-${sectionName}` : className;
}

/** Today as YYYY-MM-DD for date inputs. */
export function today(): string {
  return new Date().toISOString().slice(0, 10);
}
