import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { api } from "@/api/endpoints";
import type { ParentConversation, ParentConversationRow } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useSession } from "@/app/session";
import { cn } from "@/lib/cn";
import { errorMessage, useAsync } from "@/lib/useAsync";

const MAX_CHARS = 2000;
const keyOf = (c: { student_id: string; guardian_id: string }) =>
  `${c.student_id}/${c.guardian_id}`;

/**
 * Conversations between parents and class teachers. A class teacher sees and answers the
 * conversations of their own sections; someone who may view all (the principal) reads every
 * conversation and answers none unless they are that section's class teacher.
 */
export function MessagesTab({ onRead }: { onRead: () => void }) {
  const { t, formatDate } = useI18n();
  const { can } = useSession();
  const list = useAsync(() => api.parentConversations(), []);
  const [selected, setSelected] = useState<string | null>(null);
  const rows = list.data ?? [];
  const current = rows.find((r) => keyOf(r) === selected) ?? null;
  const when = (value: string) => formatDate(value, { dateStyle: "medium", timeStyle: "short" });
  return (
    <Card
      actions={
        <Button
          size="sm"
          variant="secondary"
          icon={<RefreshCw className="h-3.5 w-3.5" aria-hidden />}
          onClick={() => void list.reload()}
          data-testid="refresh-messages"
        >
          {t("pa.msg.refresh")}
        </Button>
      }
    >
      <p className="mb-3 text-xs text-muted">
        {can("messages.view_all") ? t("pa.msg.hint.readonly") : t("pa.msg.hint.teacher")}
      </p>
      {list.loading && !list.data ? (
        <Spinner />
      ) : list.error ? (
        <EmptyState title={errorMessage(list.error, t("app.error.generic"))} />
      ) : rows.length === 0 ? (
        <EmptyState title={t("pa.msg.empty")} description={t("pa.msg.empty.body")} />
      ) : (
        <div className="grid min-h-[22rem] grid-cols-[minmax(14rem,18rem)_1fr] gap-3">
          <ul
            aria-label={t("pa.tab.messages")}
            className="scrollbar-thin flex max-h-[32rem] flex-col divide-y divide-border overflow-y-auto rounded border border-border"
          >
            {rows.map((r) => (
              <li key={keyOf(r)}>
                <button
                  type="button"
                  onClick={() => setSelected(keyOf(r))}
                  aria-current={selected === keyOf(r)}
                  className={cn(
                    "flex w-full flex-col gap-0.5 px-3 py-2 text-start hover:bg-hover",
                    selected === keyOf(r) && "bg-selection hover:bg-selection",
                  )}
                  data-testid={`conversation-${r.student_id}`}
                >
                  <span className="flex items-center justify-between gap-2">
                    <span className="truncate text-sm font-medium text-text">{r.student_name}</span>
                    {r.unread > 0 && (
                      <Badge tone="info" aria-label={t("pa.msg.unread", { count: r.unread })}>
                        {r.unread}
                      </Badge>
                    )}
                  </span>
                  <span className="truncate text-xs text-muted">
                    {r.guardian_name}
                    {r.class_name &&
                      ` · ${r.class_name}${r.section_name ? `-${r.section_name}` : ""}`}
                  </span>
                  <span className="truncate text-xs text-text/80">{r.last_preview}</span>
                  <span className="text-2xs text-muted">{when(r.last_at)}</span>
                </button>
              </li>
            ))}
          </ul>
          {current ? (
            <Thread
              key={keyOf(current)}
              row={current}
              onChanged={() => {
                void list.reload();
                onRead();
              }}
            />
          ) : (
            <div className="flex items-center justify-center rounded border border-dashed border-border text-sm text-muted">
              {t("pa.msg.choose")}
            </div>
          )}
        </div>
      )}
    </Card>
  );
}

function Thread({ row, onChanged }: { row: ParentConversationRow; onChanged: () => void }) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const thread = useAsync<ParentConversation>(
    () => api.parentConversation(row.student_id, row.guardian_id),
    [row.student_id, row.guardian_id],
  );
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const marked = useRef(false);
  const end = useRef<HTMLDivElement>(null);
  const data = thread.data;

  // Opening a conversation you may answer marks it read, once.
  useEffect(() => {
    if (!data || marked.current || !data.can_reply || data.unread === 0) return;
    marked.current = true;
    api.markParentConversationRead(row.student_id, row.guardian_id).then(onChanged, () => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data]);
  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [data?.messages.length]);

  const send = async () => {
    const body = text.trim();
    if (!body) return;
    setBusy(true);
    try {
      await api.sendParentMessage(row.student_id, row.guardian_id, body);
      setText("");
      toast.push({ kind: "success", title: t("pa.msg.sent") });
      await thread.reload();
      onChanged();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };

  if (thread.loading && !data) return <Spinner />;
  if (!data) return <EmptyState title={errorMessage(thread.error, t("app.error.generic"))} />;
  return (
    <section
      aria-label={t("pa.msg.about", { student: data.student_name })}
      className="flex min-w-0 flex-col rounded border border-border"
    >
      <header className="border-b border-border px-3 py-2">
        <div className="text-sm font-semibold text-text">
          {t("pa.msg.about", { student: data.student_name })}
        </div>
        <div className="text-xs text-muted">
          {data.guardian_name}
          {data.class_name &&
            ` · ${data.class_name}${data.section_name ? `-${data.section_name}` : ""}`}
          {" · "}
          {data.teacher_name
            ? t("pa.msg.teacher", { name: data.teacher_name })
            : t("pa.msg.noTeacher")}
        </div>
      </header>
      <ol className="scrollbar-thin flex max-h-[24rem] flex-1 flex-col gap-2 overflow-y-auto p-3">
        {data.messages.map((m) => (
          <li
            key={m.id}
            className={cn(
              "max-w-[80%] rounded px-3 py-2 text-sm",
              m.sender === "staff"
                ? "self-end bg-selection text-text"
                : "self-start bg-hover text-text",
            )}
            data-testid={`message-${m.sender}`}
          >
            <div className="mb-0.5 text-2xs text-muted">
              {m.sender_name} · {formatDate(m.sent_at, { dateStyle: "medium", timeStyle: "short" })}
            </div>
            <div className="whitespace-pre-wrap break-words">{m.body}</div>
          </li>
        ))}
        <div ref={end} />
      </ol>
      {data.can_reply ? (
        <form
          className="flex items-end gap-2 border-t border-border p-3"
          onSubmit={(e) => {
            e.preventDefault();
            void send();
          }}
        >
          <label className="flex flex-1 flex-col text-xs font-medium text-muted">
            {t("pa.msg.write")}
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={2}
              maxLength={MAX_CHARS}
              className="mt-1 rounded border border-border bg-card px-2 py-1 text-sm text-text"
              data-testid="reply-text"
            />
          </label>
          <Button type="submit" loading={busy} disabled={!text.trim()} data-testid="send-reply">
            {t("pa.msg.send")}
          </Button>
        </form>
      ) : (
        <p className="border-t border-border px-3 py-2 text-xs text-muted" data-testid="read-only">
          {t("pa.msg.readonly")}
        </p>
      )}
    </section>
  );
}
