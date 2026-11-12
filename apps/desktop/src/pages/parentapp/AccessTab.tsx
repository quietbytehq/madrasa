import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Printer } from "lucide-react";
import { api } from "@/api/endpoints";
import type { GuardianAccessRow, GuardianView, ParentAccessCode } from "@/api/types";
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
import { useSchool } from "@/app/session";
import { errorMessage, useAsync } from "@/lib/useAsync";

/** Who has access to the parent app, and giving or ending it. */
export function AccessTab() {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const list = useAsync(() => api.parentAccess(), []);
  const [searching, setSearching] = useState(false);
  const [issued, setIssued] = useState<ParentAccessCode | null>(null);
  const [ending, setEnding] = useState<GuardianAccessRow | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const issue = async (guardianId: string) => {
    setBusy(guardianId);
    try {
      setIssued(await api.parentAccessCode(guardianId));
      setSearching(false);
      void list.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  const end = async (row: GuardianAccessRow) => {
    setBusy(row.guardian_id);
    try {
      await api.parentAccessEnd(row.guardian_id);
      toast.push({ kind: "success", title: t("pa.access.ended") });
      setEnding(null);
      void list.reload();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  const day = (value: string | null) => (value ? formatDate(value, { dateStyle: "medium" }) : "—");

  return (
    <Card
      actions={
        <Button size="sm" onClick={() => setSearching(true)} data-testid="give-access">
          {t("pa.access.give")}
        </Button>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("pa.access.hint")}</p>
      {list.loading && !list.data ? (
        <Spinner />
      ) : list.error ? (
        <EmptyState title={errorMessage(list.error, t("app.error.generic"))} />
      ) : (list.data ?? []).length === 0 ? (
        <EmptyState title={t("pa.access.empty")} />
      ) : (
        <Table<GuardianAccessRow>
          rows={list.data ?? []}
          rowKey={(r) => r.guardian_id}
          pageSize={0}
          columns={[
            { key: "guardian_name", header: t("pa.access.guardian"), sortable: true },
            {
              key: "children",
              header: t("pa.access.children"),
              render: (r) => r.children.join(", "),
            },
            {
              key: "state",
              header: t("pa.access.state"),
              render: (r) =>
                r.state === "active" ? (
                  <Badge tone="success">{t("pa.access.state.active")}</Badge>
                ) : (
                  <span className="flex flex-col gap-0.5">
                    <Badge tone="warning" className="self-start">
                      {t("pa.access.state.code")}
                    </Badge>
                    {r.code_expires_at && (
                      <span className="text-2xs text-muted">
                        {t("pa.access.codeUntil", { date: day(r.code_expires_at) })}
                      </span>
                    )}
                  </span>
                ),
            },
            {
              key: "phone_hint",
              header: t("pa.access.phone"),
              render: (r) => <span dir="ltr">{r.phone_hint ?? "—"}</span>,
            },
            { key: "since", header: t("pa.access.since"), render: (r) => day(r.since) },
            {
              key: "last_login_at",
              header: t("pa.access.lastLogin"),
              render: (r) => day(r.last_login_at),
            },
            {
              key: "guardian_id",
              header: "",
              align: "end",
              render: (r) => (
                <span className="flex justify-end gap-1.5">
                  <Button
                    size="sm"
                    variant="secondary"
                    loading={busy === r.guardian_id}
                    onClick={() => void issue(r.guardian_id)}
                    title={t("pa.access.newCode.hint")}
                    data-testid={`new-code-${r.guardian_id}`}
                  >
                    {t("pa.access.newCode")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setEnding(r)}
                    data-testid={`end-access-${r.guardian_id}`}
                  >
                    {t("pa.access.end")}
                  </Button>
                </span>
              ),
            },
          ]}
        />
      )}
      {searching && (
        <GuardianSearch
          busy={busy}
          onChoose={(g) => void issue(g.id)}
          onClose={() => setSearching(false)}
        />
      )}
      {issued && <AccessSlip code={issued} onClose={() => setIssued(null)} />}
      {ending && (
        <Dialog
          open
          size="sm"
          onClose={() => setEnding(null)}
          title={t("pa.access.end.title", { name: ending.guardian_name })}
          footer={
            <>
              <Button variant="secondary" onClick={() => setEnding(null)}>
                {t("action.cancel")}
              </Button>
              <Button
                variant="danger"
                loading={busy === ending.guardian_id}
                onClick={() => void end(ending)}
                data-testid="confirm-end-access"
              >
                {t("pa.access.end")}
              </Button>
            </>
          }
        >
          <p className="text-sm text-text">{t("pa.access.end.body")}</p>
        </Dialog>
      )}
    </Card>
  );
}

/** Find a guardian by name or phone and give them a code. */
function GuardianSearch({
  busy,
  onChoose,
  onClose,
}: {
  busy: string | null;
  onChoose: (guardian: GuardianView) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [q, setQ] = useState("");
  const [found, setFound] = useState<GuardianView[] | null>(null);
  useEffect(() => {
    const query = q.trim();
    if (query.length < 2) {
      setFound(null);
      return;
    }
    let stale = false;
    const timer = setTimeout(() => {
      api.searchGuardians(query).then(
        (rows) => !stale && setFound(rows),
        () => !stale && setFound([]),
      );
    }, 250);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [q]);
  return (
    <Dialog open onClose={onClose} title={t("pa.access.search.title")}>
      <Input
        label={t("pa.access.search.label")}
        hint={t("pa.access.search.hint")}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        autoFocus
        data-testid="guardian-search"
      />
      {found && found.length === 0 && (
        <p className="mt-3 text-sm text-muted">{t("pa.access.search.none")}</p>
      )}
      {found && found.length > 0 && (
        <ul className="mt-3 flex max-h-72 flex-col divide-y divide-border overflow-y-auto rounded border border-border">
          {found.map((g) => (
            <li key={g.id} className="flex items-center justify-between gap-3 px-3 py-2">
              <span className="min-w-0">
                <span className="block truncate text-sm font-medium text-text">{g.full_name}</span>
                <span className="block truncate text-xs text-muted">
                  {g.students.length > 0
                    ? g.students.map((s) => s.full_name).join(", ")
                    : t("pa.access.search.noChildren")}
                  {g.phone && (
                    <>
                      {" · "}
                      <span dir="ltr">{g.phone}</span>
                    </>
                  )}
                </span>
              </span>
              <Button
                size="sm"
                disabled={g.students.length === 0}
                loading={busy === g.id}
                onClick={() => onChoose(g)}
                data-testid={`choose-${g.id}`}
              >
                {t("pa.access.choose")}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}

/** What the slip says: who it is for, the address, the code, and how to start. */
function SlipText({ code }: { code: ParentAccessCode }) {
  const { t, formatDate } = useI18n();
  const school = useSchool();
  return (
    <>
      <div className="text-base font-semibold text-navy">{school.name}</div>
      <div className="mb-3 text-xs text-muted">{t("pa.slip.title")}</div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5">
        <dt className="text-muted">{t("pa.slip.for")}</dt>
        <dd className="font-medium">{code.guardian_name}</dd>
        <dt className="text-muted">{t("pa.slip.address")}</dt>
        <dd dir="ltr" className="break-all text-start font-mono">
          {code.hub_url}
        </dd>
        <dt className="text-muted">{t("pa.slip.code")}</dt>
        <dd
          dir="ltr"
          className="text-start font-mono text-lg font-semibold tracking-widest"
          data-testid="access-code"
        >
          {code.code}
        </dd>
        <dt className="text-muted">{t("pa.slip.validUntil")}</dt>
        <dd>{formatDate(code.expires_at, { dateStyle: "long" })}</dd>
      </dl>
      <div className="mt-4 font-medium">{t("pa.slip.steps.title")}</div>
      <ol className="mt-1 list-decimal ps-5">
        <li>{t("pa.slip.step1")}</li>
        <li>{t("pa.slip.step2")}</li>
        <li>{t("pa.slip.step3")}</li>
      </ol>
      <p className="mt-3 text-xs text-muted">{t("pa.slip.keepSafe")}</p>
    </>
  );
}

/**
 * The code, shown once, with a slip to print and hand to the guardian. The printed copy is
 * a second copy placed straight under the page, so it prints at the top of the sheet and
 * not where the dialog happens to sit; on screen only the one in the dialog is shown.
 */
function AccessSlip({ code, onClose }: { code: ParentAccessCode; onClose: () => void }) {
  const { t } = useI18n();
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("pa.slip.title")}
      footer={
        <>
          <Button
            variant="secondary"
            icon={<Printer className="h-4 w-4" aria-hidden />}
            onClick={() => window.print()}
          >
            {t("action.print")}
          </Button>
          <Button onClick={onClose}>{t("action.close")}</Button>
        </>
      }
    >
      <p className="mb-3 rounded bg-warning-bg px-3 py-2 text-xs text-text">{t("pa.slip.once")}</p>
      <div className="text-sm text-text" data-testid="access-slip">
        <SlipText code={code} />
      </div>
      {createPortal(
        <div
          className="print-area hidden text-sm text-text print:block"
          data-testid="access-slip-print"
        >
          <SlipText code={code} />
        </div>,
        document.body,
      )}
    </Dialog>
  );
}
