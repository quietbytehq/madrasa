import type { SchoolSummary } from "@/api/types";

/** What the UI assumes before the school profile has loaded (a Pakistani school). */
export const DEFAULT_SCHOOL: SchoolSummary = {
  id: "",
  name: "",
  code: "",
  city: "",
  is_demo: false,
  academic_year_start_month: 4,
  currency: "PKR",
  currency_symbol: "Rs",
  country: "PK",
  country_name: "Pakistan",
  time_zone: "Asia/Karachi",
  id_label: "CNIC / B-Form",
  date_order: "day_first",
};

/** Example values shown in placeholders, by country. */
export function placeholders(school: Pick<SchoolSummary, "country">): {
  id: string;
  phone: string;
} {
  return school.country === "PK"
    ? { id: "42101-1234567-1", phone: "0300-1234567" }
    : { id: "", phone: "" };
}
