import { useState } from "react";
import { api } from "@/api/endpoints";
import type { CopyLookup, LibraryMember, LoanView, MemberPage } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { useMoney } from "@/lib/useMoney";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { CopyStatusBadge, fineDue, isOverdue } from "./common";

/** The issue/return desk: look up a copy, pick a member, issue, return, renew, fines. */
export function DeskTab({ manage }: { manage: boolean }) {
  const { t, formatDate } = useI18n();
  const money = useMoney();
  const toast = useToast();
  const [accession, setAccession] = useState("");
  const [copy, setCopy] = useState<CopyLookup | null>(null);
  const [query, setQuery] = useState("");
  const [member, setMember] = useState<LibraryMember | null>(null);
  const page = useAsync(
    () => (member ? api.libraryMember(member.kind, member.id) : Promise.resolve(null)),
    [member?.kind, member?.id],
  );
  const matches = useAsync(
    () =>
      query.trim().length >= 2 ? api.libraryMembers({ q: query.trim() }) : Promise.resolve([]),
    [query],
  );
  const [busy, setBusy] = useState(false);
  const [lost, setLost] = useState<LoanView | null>(null);
  const [waiving, setWaiving] = useState<LoanView | null>(null);
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const lookup = async () => {
    if (!accession.trim()) return;
    setBusy(true);
    try {
      setCopy(await api.lookupCopy(accession.trim()));
    } catch {
      setCopy(null);
      toast.push({ kind: "warning", title: t("lib.desk.notFound") });
    } finally {
      setBusy(false);
    }
  };
  const refresh = async () => {
    if (copy) {
      try {
        setCopy(await api.lookupCopy(copy.accession_no));
      } catch {
        setCopy(null);
      }
    }
    await page.reload();
  };
  const issue = async () => {
    if (!copy || !member) return;
    setBusy(true);
    try {
      const p = await api.issueBook({
        copy_id: copy.id,
        member_kind: member.kind,
        member_id: member.id,
      });
      page.setData(p);
      const loan = p.loans.find((l) => l.copy_id === copy.id && !l.returned_on);
      toast.push({
        kind: "success",
        title: t("lib.desk.issued", {
          name: member.name,
          date: loan ? formatDate(loan.due_on) : "",
        }),
      });
      setCopy(await api.lookupCopy(copy.accession_no));
      setAccession("");
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const doReturn = async (loan: LoanView, lostCharge?: number) => {
    setBusy(true);
    try {
      const r = await api.returnLoan(
        loan.id,
        lostCharge === undefined ? {} : { lost: true, lost_charge: lostCharge },
      );
      toast.push({
        kind: "success",
        title: t("lib.desk.returned", { fine: money(Number(r.fine)) }),
      });
      setLost(null);
      await refresh();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const renew = async (loan: LoanView) => {
    setBusy(true);
    try {
      const r = await api.renewLoan(loan.id);
      toast.push({ kind: "success", title: t("lib.desk.renewed", { date: formatDate(r.due_on) }) });
      await refresh();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const settle = async (loan: LoanView) => {
    setBusy(true);
    try {
      const r = await api.settleFine(loan.id);
      toast.push({
        kind: "success",
        title: t(r.voucher_id ? "lib.fine.settledVoucher" : "lib.fine.settled"),
      });
      await refresh();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const data: MemberPage | null = page.data;
  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <Card title={t("lib.tab.desk")}>
        <p className="mb-3 text-xs text-muted">{t("lib.desk.hint")}</p>
        <form
          className="flex flex-wrap items-end gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void lookup();
          }}
        >
          <Input
            label={t("lib.desk.accession")}
            value={accession}
            onChange={(e) => setAccession(e.target.value)}
            className="w-48"
            autoFocus
          />
          <Button type="submit" loading={busy} variant="secondary">
            {t("lib.desk.lookup")}
          </Button>
        </form>
        {copy && (
          <div className="mt-3 rounded border border-border p-3 text-sm" data-testid="copy">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-medium">{copy.title}</span>
              <span className="text-muted">{copy.author}</span>
              <Badge tone="neutral">{copy.accession_no}</Badge>
              <CopyStatusBadge status={copy.status} />
            </div>
            {copy.loan ? (
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <span>
                  {t("lib.desk.onLoanTo")}{" "}
                  <span className="font-medium">{copy.loan.member_name}</span>{" "}
                  <span className="text-muted">{copy.loan.member_detail}</span>
                </span>
                <span className={isOverdue(copy.loan) ? "font-medium text-danger" : "text-muted"}>
                  {t("lib.desk.due")} {formatDate(copy.loan.due_on)}
                  {isOverdue(copy.loan) && ` · ${t("lib.desk.overdue")}`}
                </span>
                {manage && (
                  <span className="ms-auto flex gap-1">
                    <Button
                      size="sm"
                      loading={busy}
                      onClick={() => void doReturn(copy.loan!)}
                      data-testid="return"
                    >
                      {t("lib.desk.return")}
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => void renew(copy.loan!)}
                    >
                      {t("lib.desk.renew")}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={busy}
                      onClick={() => setLost(copy.loan)}
                    >
                      {t("lib.desk.lost")}
                    </Button>
                  </span>
                )}
              </div>
            ) : (
              manage &&
              copy.status === "available" && (
                <div className="mt-2 flex items-center gap-2">
                  <span className="text-muted">
                    {member ? `${t("lib.desk.member")}: ${member.name}` : t("lib.desk.pick")}
                  </span>
                  <Button
                    size="sm"
                    className="ms-auto"
                    loading={busy}
                    disabled={!member}
                    onClick={() => void issue()}
                    data-testid="issue"
                  >
                    {t("lib.desk.issue")}
                  </Button>
                </div>
              )
            )}
          </div>
        )}
      </Card>
      <Card title={t("lib.desk.member")}>
        <Input
          label={t("lib.desk.memberSearch")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-full"
        />
        {query.trim().length >= 2 && !member && (
          <ul
            className="mt-1 max-h-40 overflow-y-auto rounded border border-border text-sm"
            data-testid="members"
          >
            {(matches.data ?? []).map((m) => (
              <li key={`${m.kind}-${m.id}`}>
                <button
                  type="button"
                  className="w-full px-2 py-1 text-start hover:bg-selection"
                  onClick={() => {
                    setMember(m);
                    setQuery("");
                  }}
                >
                  <span className="font-medium">{m.name}</span>{" "}
                  <span className="text-xs text-muted">
                    {m.number} · {m.detail || t(`lib.member.${m.kind}`)}
                  </span>
                </button>
              </li>
            ))}
            {matches.data?.length === 0 && <li className="px-2 py-1 text-xs text-muted">—</li>}
          </ul>
        )}
        {member && data && (
          <div className="mt-3 text-sm" data-testid="member">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-medium">{data.member.name}</span>
              <span className="text-muted">
                {data.member.number} · {data.member.detail || t(`lib.member.${data.member.kind}`)}
              </span>
              <Badge tone={data.overdue > 0 ? "danger" : "info"}>
                {t("lib.member.open", { n: data.open })}
                {data.overdue > 0 && ` · ${t("lib.member.overdue", { n: data.overdue })}`}
              </Badge>
              {Number(data.fines_due) > 0 && (
                <Badge tone="warning">
                  {t("lib.member.fines", { amount: money(Number(data.fines_due)) })}
                </Badge>
              )}
              <span className="text-xs text-muted">
                {t("lib.member.limit", { n: data.rules.max_books, d: data.rules.loan_days })}
              </span>
              <Button size="sm" variant="ghost" className="ms-auto" onClick={() => setMember(null)}>
                {t("action.clear")}
              </Button>
            </div>
            <div className="mt-2">
              {data.loans.length === 0 ? (
                <EmptyState title={t("lib.member.empty")} />
              ) : (
                <Table<LoanView>
                  rows={data.loans}
                  rowKey={(l) => l.id}
                  pageSize={10}
                  columns={[
                    {
                      key: "title",
                      header: t("lib.book.title"),
                      render: (l) => (
                        <span>
                          <span className="font-medium">{l.title}</span>{" "}
                          <span className="text-xs text-muted">{l.accession_no}</span>
                        </span>
                      ),
                    },
                    {
                      key: "due",
                      header: t("lib.desk.due"),
                      render: (l) =>
                        l.returned_on ? (
                          <span className="text-muted">
                            {t("lib.loans.returned")} {formatDate(l.returned_on)}
                          </span>
                        ) : (
                          <span className={isOverdue(l) ? "font-medium text-danger" : ""}>
                            {formatDate(l.due_on)}
                          </span>
                        ),
                    },
                    {
                      key: "fine",
                      header: t("lib.fine.due"),
                      align: "end",
                      render: (l) =>
                        fineDue(l) > 0 ? (
                          money(fineDue(l))
                        ) : Number(l.fine) > 0 ? (
                          <span className="text-muted">{money(Number(l.fine))}</span>
                        ) : (
                          ""
                        ),
                    },
                    {
                      key: "actions",
                      header: "",
                      render: (l) =>
                        manage && (
                          <div className="flex justify-end gap-1">
                            {!l.returned_on && (
                              <>
                                <Button
                                  size="sm"
                                  variant="secondary"
                                  disabled={busy}
                                  onClick={() => void doReturn(l)}
                                >
                                  {t("lib.desk.return")}
                                </Button>
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  disabled={busy}
                                  onClick={() => void renew(l)}
                                >
                                  {t("lib.desk.renew")}
                                </Button>
                              </>
                            )}
                            {fineDue(l) > 0 && (
                              <>
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  disabled={busy}
                                  onClick={() => setWaiving(l)}
                                >
                                  {t("lib.fine.waive")}
                                </Button>
                                <Button size="sm" disabled={busy} onClick={() => void settle(l)}>
                                  {t("lib.fine.settle")}
                                </Button>
                              </>
                            )}
                          </div>
                        ),
                    },
                  ]}
                />
              )}
            </div>
          </div>
        )}
      </Card>
      {lost && (
        <LostDialog
          loan={lost}
          busy={busy}
          onClose={() => setLost(null)}
          onConfirm={(charge) => void doReturn(lost, charge)}
        />
      )}
      {waiving && (
        <WaiveDialog
          loan={waiving}
          busy={busy}
          onClose={() => setWaiving(null)}
          onConfirm={async (amount, reason) => {
            setBusy(true);
            try {
              await api.waiveFine(waiving.id, amount, reason);
              toast.push({ kind: "success", title: t("lib.fine.waived") });
              setWaiving(null);
              await refresh();
            } catch (err) {
              fail(err);
            } finally {
              setBusy(false);
            }
          }}
        />
      )}
    </div>
  );
}

function LostDialog({
  loan,
  busy,
  onClose,
  onConfirm,
}: {
  loan: LoanView;
  busy: boolean;
  onClose: () => void;
  onConfirm: (charge: number) => void;
}) {
  const { t } = useI18n();
  const [charge, setCharge] = useState("0");
  return (
    <Dialog
      open
      onClose={onClose}
      title={`${t("lib.desk.lost")}: ${loan.title}`}
      size="sm"
      footer={
        <div className="flex gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button loading={busy} onClick={() => onConfirm(Number(charge) || 0)}>
            {t("lib.desk.lost")}
          </Button>
        </div>
      }
    >
      <Input
        label={t("lib.desk.lostCharge")}
        type="number"
        min={0}
        step="0.01"
        value={charge}
        onChange={(e) => setCharge(e.target.value)}
      />
    </Dialog>
  );
}

export function WaiveDialog({
  loan,
  busy,
  onClose,
  onConfirm,
}: {
  loan: LoanView;
  busy: boolean;
  onClose: () => void;
  onConfirm: (amount: number, reason: string) => void;
}) {
  const { t } = useI18n();
  const [amount, setAmount] = useState(String(fineDue(loan)));
  const [reason, setReason] = useState("");
  return (
    <Dialog
      open
      onClose={onClose}
      title={`${t("lib.fine.waive")}: ${loan.title}`}
      size="sm"
      footer={
        <div className="flex gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!reason.trim()}
            onClick={() => onConfirm(Number(amount) || 0, reason)}
          >
            {t("lib.fine.waive")}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <Input
          label={t("lib.fine.waiveAmount")}
          type="number"
          min={0}
          step="0.01"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
        <Input
          label={t("lib.fine.waiveReason")}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
      </div>
    </Dialog>
  );
}
