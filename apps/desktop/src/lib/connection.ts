import type { NexusLinkStatus } from "@/api/types";
import type { TFunction, TranslationKey } from "@/i18n";

/** What a waiting change is, in words, from its method and path. */
const CHANGES: Array<[string, RegExp, TranslationKey]> = [
  ["POST", /^\/api\/v1\/students$/, "offline.change.studentNew"],
  ["*", /^\/api\/v1\/students\//, "offline.change.student"],
  ["POST", /^\/api\/v1\/fees\/vouchers\/[^/]+\/payments$/, "offline.change.payment"],
  ["*", /^\/api\/v1\/fees\//, "offline.change.fees"],
  ["*", /^\/api\/v1\/attendance\/staff/, "offline.change.staffAttendance"],
  ["*", /^\/api\/v1\/attendance\//, "offline.change.attendance"],
  ["*", /^\/api\/v1\/exams\//, "offline.change.marks"],
  ["*", /^\/api\/v1\/guardians/, "offline.change.guardian"],
  ["*", /^\/api\/v1\/staff/, "offline.change.staff"],
  ["*", /^\/api\/v1\/leave/, "offline.change.leave"],
  ["*", /^\/api\/v1\/admissions/, "offline.change.admissions"],
  ["*", /^\/api\/v1\/library/, "offline.change.library"],
];

export function describeChange(t: TFunction, method: string, path: string): string {
  const bare = path.split("?")[0] ?? path;
  for (const [m, re, key] of CHANGES) {
    if ((m === "*" || m === method) && re.test(bare)) return t(key);
  }
  return t("offline.change.other", { what: bare.replace(/^\/api\/v1\//, "") });
}

/** The hub's sync in a few words, or null when not joined. */
export function hubSummary(t: TFunction, hub: NexusLinkStatus | null): string | null {
  if (!hub?.linked) return null;
  if (hub.last_error) return t("offline.hub.unreachable");
  const parked = hub.parked.length + (hub.last_report?.parked_at_hub.length ?? 0);
  if (parked > 0) return t("offline.hub.attention", { count: parked });
  if (hub.pending > 0) return t("offline.hub.pending", { count: hub.pending });
  return t("offline.hub.inStep");
}
