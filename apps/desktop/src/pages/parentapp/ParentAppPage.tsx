import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "@/api/endpoints";
import { Badge } from "@/components/ui/Badge";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSession } from "@/app/session";
import { announceParentAppChange } from "@/app/parentApp";
import { useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";
import { AccessTab } from "./AccessTab";
import { MessagesTab } from "./MessagesTab";
import { NoticesTab } from "./NoticesTab";

type TabKey = "messages" | "notices" | "access";

const LABELS: Record<TabKey, TranslationKey> = {
  messages: "pa.tab.messages",
  notices: "pa.tab.notices",
  access: "pa.tab.access",
};

/**
 * The school's side of the parent app (docs/spec.md §17): conversations with parents,
 * notices, and who has access. Each tab needs its own permission; the whole area needs
 * the school to be joined to a school network, because parents reach the school through it.
 */
export function ParentAppPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const status = useAsync(() => api.parentAppStatus(), []);
  const tabs: TabKey[] = [];
  if (can("messages.reply") || can("messages.view_all")) tabs.push("messages");
  if (can("notices.manage")) tabs.push("notices");
  if (can("parents.manage")) tabs.push("access");
  const [params, setParams] = useSearchParams();
  const asked = params.get("tab") as TabKey | null;
  const [tab, setTab] = useState<TabKey | undefined>(
    asked && tabs.includes(asked) ? asked : tabs[0],
  );
  const select = (key: TabKey) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    setParams(next, { replace: true });
  };
  const header = (
    <PageHeader
      title={t("pa.title")}
      crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("pa.title") }]}
    />
  );
  if (status.loading && !status.data) {
    return (
      <>
        {header}
        <Spinner />
      </>
    );
  }
  if (!status.data?.joined) {
    return (
      <>
        {header}
        <Card>
          <EmptyState title={t("pa.notJoined.title")} description={t("pa.notJoined.body")} />
        </Card>
      </>
    );
  }
  const unread = status.data.unread;
  return (
    <>
      {header}
      <div
        role="tablist"
        aria-label={t("pa.title")}
        className="mb-3 flex gap-1 border-b border-border print:hidden"
      >
        {tabs.map((key) => (
          <button
            key={key}
            role="tab"
            type="button"
            aria-selected={tab === key}
            onClick={() => select(key)}
            className={`-mb-px flex items-center gap-1.5 border-b-2 px-3 py-2 text-sm ${
              tab === key
                ? "border-blue font-medium text-blue"
                : "border-transparent text-muted hover:text-text"
            }`}
          >
            {t(LABELS[key])}
            {key === "messages" && unread > 0 && (
              <Badge tone="info" data-testid="tab-unread">
                {unread}
              </Badge>
            )}
          </button>
        ))}
      </div>
      <p className="mb-3 text-xs text-muted print:hidden">{t("pa.syncHint")}</p>
      {tab === "messages" && (
        <MessagesTab
          onRead={() => {
            void status.reload();
            announceParentAppChange();
          }}
        />
      )}
      {tab === "notices" && <NoticesTab />}
      {tab === "access" && <AccessTab />}
    </>
  );
}
