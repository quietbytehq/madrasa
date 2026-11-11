import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { hub } from "../api";
import { brief, td, th, useWhen } from "../format";
import { PageTitle } from "./common";

/** The hub's activity log: every change made here, by whom and when. Nothing is removed. */
export function ActivityPage() {
  const { t } = useI18n();
  const when = useWhen();
  const [page, setPage] = useState(1);
  const log = useAsync(() => hub.audit(page), [page]);
  const users = useAsync(() => hub.users(), []);
  const who = (id: string | null) =>
    id ? (users.data?.find((u) => u.id === id)?.full_name ?? "—") : t("hub.activity.school");

  return (
    <>
      <PageTitle title={t("hub.nav.activity")} about={t("hub.activity.about")} />
      <Card padded={false}>
        {log.loading && !log.data ? (
          <div className="p-4">
            <Spinner />
          </div>
        ) : !log.data?.length ? (
          <p className="p-4 text-sm text-muted">{t("hub.activity.none")}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="border-b border-border">
                <tr>
                  <th className={th}>{t("nexus.log.when")}</th>
                  <th className={th}>{t("hub.activity.who")}</th>
                  <th className={th}>{t("hub.activity.what")}</th>
                  <th className={th}>{t("hub.activity.details")}</th>
                </tr>
              </thead>
              <tbody>
                {log.data.map((e) => (
                  <tr key={e.id} className="border-t border-border">
                    <td className={`${td} whitespace-nowrap`}>{when(e.at)}</td>
                    <td className={td}>{who(e.actor_user_id)}</td>
                    <td className={td} dir="ltr">
                      <span className="text-start">
                        {e.action} · {e.entity}
                      </span>
                    </td>
                    <td className={`${td} break-all text-xs text-muted`} dir="ltr">
                      {brief(e.new_value)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <div className="flex justify-end gap-2 border-t border-border px-3 py-2">
          <Button variant="ghost" size="sm" disabled={page <= 1} onClick={() => setPage(page - 1)}>
            {t("hub.activity.newer")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={(log.data?.length ?? 0) < 50}
            onClick={() => setPage(page + 1)}
          >
            {t("hub.activity.older")}
          </Button>
        </div>
      </Card>
    </>
  );
}
