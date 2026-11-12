import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import { RefreshCw, Send } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/cn";
import { useAsync } from "@/lib/useAsync";
import { hubApi } from "@parent/api";
import type { Message } from "@parent/api";
import { errorText, LastUpdated, Loadable } from "@parent/ui";
import { useChild } from "./Child";

const MAX_CHARS = 2000;

/**
 * The conversation with the class teacher about this child. Messages travel with the
 * school's sync, so an answer is not instant; the screen says so instead of pretending.
 */
export function MessagesScreen() {
  const { t, formatDate } = useI18n();
  const { child, overview } = useChild();
  const data = useAsync(() => hubApi.messages(child), [child]);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [sent, setSent] = useState<Message[]>([]);
  const marked = useRef(false);
  const end = useRef<HTMLDivElement>(null);
  const thread = data.data;

  // Opening the conversation is reading it: tell the hub once, then refresh the counts.
  useEffect(() => {
    if (!thread || marked.current || thread.unread === 0) return;
    marked.current = true;
    hubApi.markRead(child).then(
      () => void overview.reload(),
      () => {},
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [thread]);

  const all = thread
    ? [...thread.messages, ...sent.filter((m) => !thread.messages.some((x) => x.id === m.id))]
    : [];
  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [all.length]);

  const send = async (e: FormEvent) => {
    e.preventDefault();
    const body = text.trim();
    if (!body) return;
    setBusy(true);
    setProblem(null);
    try {
      const message = await hubApi.sendMessage(child, body);
      setSent((list) => [...list, message]);
      setText("");
    } catch (err) {
      setProblem(errorText(err, t));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Loadable state={data}>
      {(c) => (
        <div className="flex min-h-full flex-col gap-2">
          <div className="flex items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2">
            <div className="min-w-0 text-sm">
              <div className="text-xs text-muted">{t("parent.messages.teacher")}</div>
              <div className="truncate font-medium text-text" data-testid="teacher">
                {c.class_teacher ?? t("parent.messages.noTeacher")}
              </div>
            </div>
            <Button
              size="sm"
              variant="secondary"
              icon={<RefreshCw className="h-3.5 w-3.5" aria-hidden />}
              onClick={() => void data.reload()}
              loading={data.loading}
              data-testid="refresh"
            >
              {t("parent.messages.refresh")}
            </Button>
          </div>
          <LastUpdated at={c.synced_at} className="py-0" />
          {all.length === 0 ? (
            <p className="px-4 py-8 text-center text-sm text-muted">{t("parent.messages.none")}</p>
          ) : (
            <ol className="flex flex-1 flex-col gap-2" aria-label={t("parent.nav.messages")}>
              {all.map((m) => (
                <li
                  key={m.id}
                  data-testid={`message-${m.sender}`}
                  className={cn(
                    "max-w-[85%] rounded-lg px-3 py-2 text-sm",
                    m.sender === "guardian"
                      ? "self-end bg-selection text-text"
                      : "self-start border border-border bg-card text-text",
                  )}
                >
                  <div className="mb-0.5 text-2xs text-muted">
                    {m.sender === "guardian" ? t("parent.messages.you") : m.sender_name}
                    {" · "}
                    {formatDate(m.sent_at, { dateStyle: "medium", timeStyle: "short" })}
                  </div>
                  <div className="whitespace-pre-wrap break-words">{m.body}</div>
                </li>
              ))}
            </ol>
          )}
          <div ref={end} />
          {sent.length > 0 && (
            <p role="status" className="px-1 text-xs text-muted" data-testid="sent-note">
              {t("parent.messages.sentNote")}
            </p>
          )}
          {c.can_send ? (
            <form
              className="sticky bottom-0 -mx-3 flex items-end gap-2 border-t border-border bg-bg px-3 py-2"
              onSubmit={(e) => void send(e)}
            >
              <label className="flex flex-1 flex-col text-xs font-medium text-muted">
                {t("parent.messages.write")}
                <textarea
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  rows={2}
                  maxLength={MAX_CHARS}
                  className="mt-1 rounded border border-border bg-card px-2 py-1.5 text-sm text-text"
                  data-testid="message-text"
                />
              </label>
              <Button
                type="submit"
                loading={busy}
                disabled={!text.trim()}
                className="min-h-11"
                icon={<Send className="h-4 w-4 rtl:-scale-x-100" aria-hidden />}
                data-testid="send"
              >
                {t("parent.messages.send")}
              </Button>
            </form>
          ) : (
            <p
              className="rounded bg-warning-bg px-3 py-2 text-xs text-text"
              data-testid="cannot-send"
            >
              {t("parent.error.noTeacher")}
            </p>
          )}
          {problem && (
            <p role="alert" className="rounded bg-danger-bg px-3 py-2 text-sm text-danger">
              {problem}
            </p>
          )}
        </div>
      )}
    </Loadable>
  );
}
