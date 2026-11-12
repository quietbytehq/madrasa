import { useState } from "react";
import { ChevronRight, LogOut, MessageSquare, Plus } from "lucide-react";
import { Link } from "react-router-dom";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { hubApi } from "@parent/api";
import { LanguageSwitch } from "@parent/LanguageSwitch";
import { useSession } from "@parent/session";
import { LastUpdated, Loadable, Screen, useLtr } from "@parent/ui";

/** The parent's children, across every school that gave them access. */
export function Children() {
  const { t } = useI18n();
  const session = useSession();
  const ltr = useLtr();
  const list = useAsync(() => hubApi.children(), []);
  const [leaving, setLeaving] = useState(false);
  return (
    <Screen
      title={t("parent.children.title")}
      subtitle={session.name ?? undefined}
      actions={<LanguageSwitch />}
    >
      <Loadable state={list}>
        {({ children }) =>
          children.length === 0 ? (
            <div className="px-4 py-10 text-center" data-testid="no-children">
              <p className="text-sm font-medium text-text">{t("parent.children.none")}</p>
              <p className="mt-2 text-sm text-muted">{t("parent.children.none.body")}</p>
            </div>
          ) : (
            <ul className="flex flex-col gap-2">
              {children.map((c) => (
                <li key={`${c.school_ref}/${c.student_id}`}>
                  <Link
                    to={`/child/${c.school_ref}/${c.student_id}`}
                    className="flex items-center gap-3 rounded-lg border border-border bg-card p-3 shadow-card hover:bg-hover"
                    data-testid={`child-${c.student_id}`}
                  >
                    <div
                      aria-hidden
                      className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-selection text-base font-semibold text-blue"
                    >
                      {c.full_name.trim().charAt(0)}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-base font-semibold text-text">
                        {c.full_name}
                      </div>
                      <div className="truncate text-xs text-muted">
                        <bdi>
                          {c.class_name
                            ? t("parent.child.class", {
                                class: ltr(
                                  c.section_name
                                    ? `${c.class_name}-${c.section_name}`
                                    : c.class_name,
                                ),
                              })
                            : t("parent.child.noClass")}
                        </bdi>
                        {" · "}
                        <bdi>{c.school_name}</bdi>
                      </div>
                    </div>
                    {c.unread > 0 && (
                      <Badge
                        tone="info"
                        aria-label={t("parent.messages.unread", { count: c.unread })}
                      >
                        <MessageSquare className="me-1 inline h-3 w-3" aria-hidden />
                        {c.unread}
                      </Badge>
                    )}
                    <ChevronRight
                      className="h-4 w-4 shrink-0 text-muted rtl:rotate-180"
                      aria-hidden
                    />
                  </Link>
                  <LastUpdated at={c.synced_at} className="px-1 py-1" />
                </li>
              ))}
            </ul>
          )
        }
      </Loadable>
      <div className="mt-6 flex flex-col gap-2 border-t border-border pt-4">
        <Link
          to="/code"
          className="flex min-h-11 items-center gap-2 rounded px-2 text-sm font-medium text-blue hover:bg-hover"
        >
          <Plus className="h-4 w-4" aria-hidden />
          {t("parent.children.addCode")}
        </Link>
        <Button
          variant="ghost"
          className="min-h-11 justify-start px-2"
          icon={<LogOut className="h-4 w-4 rtl:rotate-180" aria-hidden />}
          loading={leaving}
          onClick={() => {
            setLeaving(true);
            void session.signOut();
          }}
          data-testid="sign-out"
        >
          {t("parent.signOut")}
        </Button>
        {session.phone && (
          <p className="px-2 text-xs text-muted">
            {t("parent.signedInAs")} <span dir="ltr">{session.phone}</span>
          </p>
        )}
      </div>
    </Screen>
  );
}
