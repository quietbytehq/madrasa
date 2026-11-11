import { useI18n } from "@/i18n";
import type { I18n } from "@/i18n";
import type { ColumnKind } from "./api";

/** A date and time in the reader's language, or a dash. */
export function useWhen(): (value: string | null | undefined) => string {
  const { formatDate } = useI18n();
  return (value) => (value ? formatDate(value, { dateStyle: "medium", timeStyle: "short" }) : "—");
}

/** Table header and cell classes shared by the console's tables. */
export const th = "px-3 py-2 text-start text-xs font-medium text-muted";
export const td = "px-3 py-2 align-top text-sm";

/**
 * Put thousands separators into an exact decimal string ("10497123.00" → "10,497,123.00").
 * Text in, text out: the amount is never turned into a floating-point number.
 */
export function groupDigits(amount: string): string {
  const match = /^(-?)(\d+)(\.\d+)?$/.exec(amount.trim());
  if (!match) return amount;
  const [, sign, whole, fraction] = match;
  return `${sign}${whole!.replace(/\B(?=(\d{3})+(?!\d))/g, ",")}${fraction ?? ""}`;
}

/** One report cell as text. Money arrives as exact text and is shown as it is, never recomputed. */
export function cell(
  value: unknown,
  kind: ColumnKind,
  i18n: Pick<I18n, "formatNumber" | "formatDate">,
): string {
  if (value === null || value === undefined || value === "") return "—";
  switch (kind) {
    case "int":
      return typeof value === "number" ? i18n.formatNumber(value) : String(value);
    case "money":
      return groupDigits(String(value));
    case "percent":
      return `${String(value)}%`;
    case "date":
      return i18n.formatDate(String(value));
    case "date_time":
      return i18n.formatDate(String(value), { dateStyle: "medium", timeStyle: "short" });
    default:
      return String(value);
  }
}

/** A logged value as one short line. */
export function brief(value: unknown): string {
  if (value === null || value === undefined) return "";
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return text.length > 160 ? `${text.slice(0, 157)}…` : text;
}
