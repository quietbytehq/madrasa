import { useState } from "react";
import { api } from "@/api/endpoints";
import type { ParentNotice, ParentNoticeBody, SchoolClass } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { todayIso } from "@/lib/dates";
import { errorMessage, useAsync } from "@/lib/useAsync";

type NoticeState = "draft" | "published" | "expired";

/** What parents see of a notice today. */
export function noticeState(notice: ParentNotice, today: string = todayIso()): NoticeState {
  if (!notice.published_at) return "draft";
  if (notice.expires_on && notice.expires_on < today) return "expired";
  return "published";
}

const STATE_TONE: Record<NoticeState, "neutral" | "success" | "warning"> = {
  draft: "neutral",
  published: "success",
  expired: "warning",
};

/** School notices for the parent app: write, publish, withdraw. */
export function NoticesTab() {
  const { t, formatDate } = useI18n();
  const notices = useAsync(() => api.parentNotices(), []);
  const classes = useAsync(() => api.classes(), []);
  const [open, setOpen] = useState<ParentNotice | "new" | null>(null);
  const className = (id: string) => (classes.data ?? []).find((c) => c.id === id)?.name ?? "—";
  return (
    <Card
      actions={
        <Button size="sm" onClick={() => setOpen("new")} data-testid="new-notice">
          {t("pa.notices.new")}
        </Button>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("pa.notices.hint")}</p>
      {notices.loading && !notices.data ? (
        <Spinner />
      ) : notices.error ? (
        <EmptyState title={errorMessage(notices.error, t("app.error.generic"))} />
      ) : (notices.data ?? []).length === 0 ? (
        <EmptyState title={t("pa.notices.empty")} />
      ) : (
        <Table<ParentNotice>
          rows={notices.data ?? []}
          rowKey={(n) => n.id}
          pageSize={0}
          onRowClick={(n) => setOpen(n)}
          columns={[
            { key: "title", header: t("pa.notices.title") },
            {
              key: "class_ids",
              header: t("pa.notices.for"),
              render: (n) =>
                n.class_ids.length === 0
                  ? t("pa.notices.wholeSchool")
                  : n.class_ids.map(className).join(", "),
            },
            {
              key: "published_at",
              header: t("pa.notices.status"),
              render: (n) => {
                const state = noticeState(n);
                return (
                  <Badge tone={STATE_TONE[state]}>
                    {t(`pa.notices.status.${state}` as TranslationKey)}
                  </Badge>
                );
              },
            },
            {
              key: "published_on",
              header: t("pa.notices.publishedOn"),
              render: (n) =>
                n.published_at ? formatDate(n.published_at, { dateStyle: "medium" }) : "—",
            },
            {
              key: "expires_on",
              header: t("pa.notices.lastDay"),
              render: (n) =>
                n.expires_on ? formatDate(n.expires_on, { dateStyle: "medium" }) : "—",
            },
          ]}
        />
      )}
      {open && (
        <NoticeDialog
          notice={open === "new" ? null : open}
          classes={classes.data ?? []}
          onClose={() => setOpen(null)}
          onChanged={() => void notices.reload()}
        />
      )}
    </Card>
  );
}

function NoticeDialog({
  notice,
  classes,
  onClose,
  onChanged,
}: {
  notice: ParentNotice | null;
  classes: SchoolClass[];
  onClose: () => void;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [title, setTitle] = useState(notice?.title ?? "");
  const [body, setBody] = useState(notice?.body ?? "");
  const [chosen, setChosen] = useState<string[]>(notice?.class_ids ?? []);
  const [whole, setWhole] = useState((notice?.class_ids.length ?? 0) === 0);
  const [lastDay, setLastDay] = useState(notice?.expires_on ?? "");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const published = !!notice?.published_at;

  const run = async (work: () => Promise<unknown>, done: TranslationKey) => {
    setBusy(true);
    setProblem(null);
    try {
      await work();
      toast.push({ kind: "success", title: t(done) });
      onChanged();
      onClose();
    } catch (err) {
      setProblem(errorMessage(err, t("app.error.generic")));
    } finally {
      setBusy(false);
    }
  };
  const form = (): ParentNoticeBody | null => {
    if (!whole && chosen.length === 0) {
      setProblem(t("pa.notices.needClass"));
      return null;
    }
    return {
      title,
      body,
      class_ids: whole ? [] : chosen,
      expires_on: lastDay || null,
    };
  };
  /** Save the text; `publish` also makes it visible to parents. */
  const save = (publish: boolean) => {
    const input = form();
    if (!input) return;
    void run(
      async () => {
        if (!notice) {
          await api.createParentNotice({ ...input, publish });
          return;
        }
        await api.updateParentNotice(notice.id, input);
        if (publish && !published) await api.publishParentNotice(notice.id);
      },
      publish && !published ? "pa.notices.published" : "pa.notices.saved",
    );
  };
  const toggle = (id: string) =>
    setChosen((list) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id]));

  return (
    <Dialog
      open
      size="lg"
      onClose={onClose}
      title={notice ? t("pa.notices.edit") : t("pa.notices.new")}
      footer={
        <>
          {notice && (
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() =>
                void run(() => api.deleteParentNotice(notice.id), "pa.notices.deleted")
              }
              data-testid="delete-notice"
            >
              {t("pa.notices.delete")}
            </Button>
          )}
          {published && notice && (
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() =>
                void run(() => api.withdrawParentNotice(notice.id), "pa.notices.withdrawn")
              }
              data-testid="withdraw-notice"
            >
              {t("pa.notices.withdraw")}
            </Button>
          )}
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            variant={published ? "primary" : "secondary"}
            disabled={busy}
            onClick={() => save(false)}
            data-testid="save-notice"
          >
            {published ? t("action.save") : t("pa.notices.saveDraft")}
          </Button>
          {!published && (
            <Button loading={busy} onClick={() => save(true)} data-testid="publish-notice">
              {t("pa.notices.publish")}
            </Button>
          )}
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Input
          label={t("pa.notices.title")}
          value={title}
          maxLength={120}
          onChange={(e) => setTitle(e.target.value)}
          data-testid="notice-title"
        />
        <label className="flex flex-col text-xs font-medium text-muted">
          {t("pa.notices.body")}
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            rows={6}
            maxLength={4000}
            className="mt-1 rounded border border-border bg-card px-2 py-1 text-sm text-text"
            data-testid="notice-body"
          />
        </label>
        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1 text-xs font-medium text-muted">{t("pa.notices.for")}</legend>
          <label className="flex items-center gap-2 text-sm text-text">
            <input
              type="radio"
              name="notice-audience"
              checked={whole}
              onChange={() => setWhole(true)}
              className="h-4 w-4 accent-blue"
            />
            {t("pa.notices.wholeSchool")}
          </label>
          <label className="flex items-center gap-2 text-sm text-text">
            <input
              type="radio"
              name="notice-audience"
              checked={!whole}
              onChange={() => setWhole(false)}
              className="h-4 w-4 accent-blue"
              data-testid="audience-classes"
            />
            {t("pa.notices.classes")}
          </label>
          {!whole && (
            <div className="ms-6 flex flex-wrap gap-x-4 gap-y-1">
              {classes.map((c) => (
                <label key={c.id} className="flex items-center gap-1.5 text-sm text-text">
                  <input
                    type="checkbox"
                    checked={chosen.includes(c.id)}
                    onChange={() => toggle(c.id)}
                    className="h-4 w-4 accent-blue"
                  />
                  {c.name}
                </label>
              ))}
            </div>
          )}
        </fieldset>
        <Input
          type="date"
          label={t("pa.notices.lastDay")}
          hint={t("pa.notices.lastDay.hint")}
          value={lastDay}
          onChange={(e) => setLastDay(e.target.value)}
          className="w-44"
        />
        {problem && (
          <p role="alert" className="text-sm text-danger">
            {problem}
          </p>
        )}
      </div>
    </Dialog>
  );
}
