import { Link } from "react-router-dom";
import { ShieldAlert } from "lucide-react";
import type { BackupStatus } from "@/api/types";
import { useI18n } from "@/i18n";

/**
 * Said plainly when the school's records are not safely backed up: the last backup failed,
 * or it was written and did not pass its check. Shows what happened, how old the last good
 * backup is, and what to do. Nothing is rendered while backups are in order.
 */
export function BackupAlert({ status, link }: { status: BackupStatus | null; link?: boolean }) {
  const { t, formatDate } = useI18n();
  if (!status) return null;
  const failed = status.last_error !== null;
  const unchecked = !failed && status.check === "failed";
  if (!failed && !unchecked) return null;
  return (
    <div
      role="alert"
      data-testid="backup-alert"
      className="mb-4 flex gap-3 rounded border border-danger bg-danger-bg px-4 py-3 text-sm text-text"
    >
      <ShieldAlert className="mt-0.5 h-5 w-5 shrink-0 text-danger" aria-hidden />
      <div className="flex flex-col gap-1">
        <p className="font-semibold text-danger">
          {t(failed ? "backup.alert.failed.title" : "backup.alert.check.title")}
        </p>
        <p>
          {failed && status.last_failed_at ? `${formatDate(status.last_failed_at)}: ` : ""}
          <span dir="auto">{failed ? status.last_error : status.check_message}</span>
        </p>
        {failed && (
          <p>
            {status.last_backup_at
              ? t("backup.alert.failed.lastGood", { date: formatDate(status.last_backup_at) })
              : t("backup.alert.failed.noGood")}
          </p>
        )}
        <p>
          {t(failed ? "backup.alert.failed.do" : "backup.alert.check.do")}{" "}
          {link && (
            <Link to="/backup" className="font-medium text-blue underline">
              {t("dashboard.backup.open")}
            </Link>
          )}
        </p>
      </div>
    </div>
  );
}
