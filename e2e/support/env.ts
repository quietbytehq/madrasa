/** Where scripts/e2e.sh puts everything. */
export const URLS = {
  /** The school server's API (the app on the server PC talks to this). */
  school: "http://127.0.0.1:47802",
  /** A client PC's proxy to the school server, with the offline queue. */
  client: "http://127.0.0.1:47803",
  hub: "http://localhost:47880",
  desktop: "http://127.0.0.1:1425",
  parent: "http://127.0.0.1:1445",
};
export const PASSWORD = process.env.E2E_PASSWORD ?? "e2e-password-1";
export const BACKUP_PASSWORD = "e2e-backup-password-1";
export const ADMIN = "admin";
export const HUB_ADMIN = "hub.admin";
export const PARENT_PHONE = "+923001234567";
/** Process id of the school server, to freeze and thaw it (a pulled cable). */
export const SCHOOL_PID = Number(process.env.E2E_SCHOOL_PID ?? 0);
