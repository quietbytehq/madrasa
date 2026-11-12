import { Badge } from "@/components/ui/Badge";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { hubApi } from "@parent/api";
import { LastUpdated, Loadable, Panel } from "@parent/ui";
import { useChild } from "./Child";

/** The school's notices: for everyone, or for this child's class. Newest first. */
export function NoticesScreen() {
  const { t, formatDate } = useI18n();
  const { child } = useChild();
  const data = useAsync(() => hubApi.notices(child), [child]);
  return (
    <Loadable state={data}>
      {({ notices, synced_at }) => (
        <div className="flex flex-col gap-2">
          {notices.length === 0 ? (
            <p className="px-4 py-10 text-center text-sm text-muted">{t("parent.notices.none")}</p>
          ) : (
            notices.map((n) => (
              <Panel key={n.id}>
                <article data-testid={`notice-${n.id}`}>
                  <div className="flex items-start justify-between gap-2">
                    <h2 className="text-base font-semibold text-text">{n.title}</h2>
                    {!n.whole_school && <Badge tone="lavender">{t("parent.notices.class")}</Badge>}
                  </div>
                  {n.published_at && (
                    <div className="text-xs text-muted">
                      {formatDate(n.published_at, { dateStyle: "long" })}
                    </div>
                  )}
                  {n.body && (
                    <p className="mt-2 whitespace-pre-wrap break-words text-sm text-text">
                      {n.body}
                    </p>
                  )}
                </article>
              </Panel>
            ))
          )}
          <LastUpdated at={synced_at} />
        </div>
      )}
    </Loadable>
  );
}
