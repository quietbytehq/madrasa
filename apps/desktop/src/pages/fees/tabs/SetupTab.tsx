import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { FeeHead, FeePolicy } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { parseMoney } from "@/lib/money";
import { useMoney } from "@/lib/useMoney";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";

const DEFAULT_HEADS: Array<[string, string, boolean]> = [
  ["Tuition", "TUI", true],
  ["Admission", "ADM", false],
  ["Transport", "TRN", true],
  ["Exam", "EXM", true],
];

export function SetupTab() {
  const { t } = useI18n();
  const { can } = useSession();
  const approve = can("fees.approve");
  const toast = useToast();
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
  const policy = useAsync(() => api.feePolicy(), []);
  const heads = useAsync(() => api.feeHeads(), []);
  return (
    <div className="grid gap-4 xl:grid-cols-2">
      {policy.data && (
        <PolicyCard
          initial={policy.data}
          canEdit={approve}
          onError={fail}
          onSaved={(p) => {
            policy.setData(p);
            toast.push({ kind: "success", title: t("common.saved") });
          }}
        />
      )}
      <HeadsCard
        heads={heads.data ?? []}
        canEdit={approve}
        onChange={() => void heads.reload()}
        onError={fail}
      />
      <StructureCard
        heads={heads.data ?? []}
        canEdit={approve}
        onError={fail}
        onSaved={() => toast.push({ kind: "success", title: t("common.saved") })}
      />
    </div>
  );
}

function PolicyCard({
  initial,
  canEdit,
  onSaved,
  onError,
}: {
  initial: FeePolicy;
  canEdit: boolean;
  onSaved: (p: FeePolicy) => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [p, setP] = useState(initial);
  const [busy, setBusy] = useState(false);
  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      onSaved(await api.updateFeePolicy({ ...p, due_day: Number(p.due_day) }));
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title={t("fees.setup.policy")}>
      <form onSubmit={save} noValidate className="grid grid-cols-2 gap-3">
        <Input
          label={t("fees.setup.siblingPercent")}
          inputMode="decimal"
          value={p.sibling_discount_percent}
          onChange={(e) => setP({ ...p, sibling_discount_percent: e.target.value })}
          disabled={!canEdit}
        />
        <Select
          label={t("fees.setup.siblingRule")}
          value={p.sibling_rule}
          onChange={(e) =>
            setP({ ...p, sibling_rule: e.target.value as FeePolicy["sibling_rule"] })
          }
          options={[
            { value: "all", label: t("fees.setup.siblingRule.all") },
            {
              value: "second_child_onwards",
              label: t("fees.setup.siblingRule.second_child_onwards"),
            },
          ]}
          disabled={!canEdit}
        />
        <Select
          label={t("fees.setup.concessionBase")}
          value={p.concession_base ?? "gross"}
          onChange={(e) =>
            setP({ ...p, concession_base: e.target.value as FeePolicy["concession_base"] })
          }
          options={[
            { value: "gross", label: t("fees.setup.concessionBase.gross") },
            {
              value: "after_sibling_discount",
              label: t("fees.setup.concessionBase.after_sibling_discount"),
            },
          ]}
          disabled={!canEdit}
          className="col-span-2"
        />
        <ConcessionBaseExample
          siblingPercent={p.sibling_discount_percent}
          base={p.concession_base ?? "gross"}
        />
        <Input
          label={t("fees.setup.latePercent")}
          inputMode="decimal"
          value={p.late_fee_percent}
          onChange={(e) => setP({ ...p, late_fee_percent: e.target.value })}
          disabled={!canEdit}
        />
        <Input
          label={t("fees.setup.dueDay")}
          type="number"
          min={1}
          max={28}
          value={p.due_day}
          onChange={(e) => setP({ ...p, due_day: Number(e.target.value) })}
          disabled={!canEdit}
        />
        {canEdit && (
          <div className="col-span-2 flex justify-end">
            <Button type="submit" loading={busy}>
              {t("action.save")}
            </Button>
          </div>
        )}
      </form>
    </Card>
  );
}

/** Short explanation plus a worked example of the two concession bases. Display only: the
 * server does the real arithmetic. */
function ConcessionBaseExample({
  siblingPercent,
  base,
}: {
  siblingPercent: string;
  base: FeePolicy["concession_base"];
}) {
  const { t } = useI18n();
  const money = useMoney();
  const fee = 3000;
  const sp = Number(siblingPercent) || 0;
  const sibling = Math.round(fee * sp) / 100;
  const afterSibling = fee - sibling;
  const onGross = Math.round(fee * 25) / 100;
  const stacked = Math.round(afterSibling * 25) / 100;
  const line = (key: FeePolicy["concession_base"], baseAmount: number, concession: number) => (
    <li className={base === key ? "font-semibold text-text" : ""}>
      {t(`fees.setup.concessionBase.example.${key}`, {
        base: money(baseAmount),
        amount: money(concession),
        net: money(afterSibling - concession),
      })}
    </li>
  );
  return (
    <div className="col-span-2 rounded border border-border bg-bg px-3 py-2 text-xs text-muted">
      <p>{t("fees.setup.concessionBase.hint")}</p>
      <p className="mt-1">
        {t("fees.setup.concessionBase.example.intro", {
          fee: money(fee),
          siblingPercent: String(sp),
          sibling: money(sibling),
        })}
      </p>
      <ul className="mt-1 list-inside list-disc">
        {line("gross", fee, onGross)}
        {line("after_sibling_discount", afterSibling, stacked)}
      </ul>
    </div>
  );
}

function HeadsCard({
  heads,
  canEdit,
  onChange,
  onError,
}: {
  heads: FeeHead[];
  canEdit: boolean;
  onChange: () => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const [recurring, setRecurring] = useState(true);
  const [busy, setBusy] = useState(false);
  const add = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    setBusy(true);
    try {
      await api.createFeeHead({
        name: name.trim(),
        code: (code.trim() || name.trim().slice(0, 3)).toUpperCase(),
        is_recurring: recurring,
        position: heads.length,
      });
      setName("");
      setCode("");
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  const addDefaults = async () => {
    setBusy(true);
    try {
      const existing = new Set(heads.map((h) => h.code));
      for (const [i, [n, c, r]] of DEFAULT_HEADS.entries())
        if (!existing.has(c))
          await api.createFeeHead({ name: n, code: c, is_recurring: r, position: i });
      onChange();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card
      title={t("fees.setup.heads")}
      padded={false}
      actions={
        canEdit &&
        heads.length === 0 && (
          <Button size="sm" variant="secondary" loading={busy} onClick={() => void addDefaults()}>
            {t("fees.setup.heads.addDefaults")}
          </Button>
        )
      }
    >
      {heads.length === 0 ? (
        <EmptyState title={t("fees.setup.heads")} description={t("fees.setup.heads.empty")} />
      ) : (
        <ul className="divide-y divide-border text-sm">
          {heads.map((h) => (
            <li key={h.id} className="flex items-center gap-3 px-4 py-2">
              <span className="font-medium text-text">{h.name}</span>
              <span className="font-mono text-xs text-muted">{h.code}</span>
              <Badge tone={h.is_recurring ? "info" : "lavender"}>
                {h.is_recurring ? t("fees.setup.heads.recurring") : t("fees.setup.heads.oneTime")}
              </Badge>
            </li>
          ))}
        </ul>
      )}
      {canEdit && (
        <form onSubmit={add} noValidate className="flex items-end gap-2 border-t border-border p-3">
          <Input
            label={t("common.name")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="w-44"
          />
          <Input
            label={t("common.code")}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            className="w-24"
          />
          <Select
            label={t("fees.tabs.setup")}
            value={recurring ? "monthly" : "once"}
            onChange={(e) => setRecurring(e.target.value === "monthly")}
            options={[
              { value: "monthly", label: t("fees.setup.heads.recurring") },
              { value: "once", label: t("fees.setup.heads.oneTime") },
            ]}
            className="w-32"
          />
          <Button type="submit" loading={busy} disabled={!name.trim()}>
            {t("action.add")}
          </Button>
        </form>
      )}
    </Card>
  );
}

function StructureCard({
  heads,
  canEdit,
  onSaved,
  onError,
}: {
  heads: FeeHead[];
  canEdit: boolean;
  onSaved: () => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const years = useAsync(() => api.academicYears(), []);
  const classes = useAsync(() => api.classes(), []);
  const [yearId, setYearId] = useState("");
  const [classId, setClassId] = useState("");
  const activeYear = useMemo(
    () => years.data?.find((y) => y.is_active)?.id ?? years.data?.[0]?.id ?? "",
    [years.data],
  );
  const selectedYear = yearId || activeYear;
  const selectedClass = classId || classes.data?.[0]?.id || "";
  const structure = useAsync(
    () =>
      selectedYear && selectedClass
        ? api.feeStructure(selectedYear, selectedClass)
        : Promise.resolve([]),
    [selectedYear, selectedClass],
  );
  const [amounts, setAmounts] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const next: Record<string, string> = {};
    for (const h of heads)
      next[h.id] = structure.data?.find((s) => s.fee_head_id === h.id)?.amount ?? "0";
    setAmounts(next);
  }, [structure.data, heads]);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    const lines = [];
    for (const h of heads) {
      const parsed = parseMoney(amounts[h.id] || "0");
      if (!parsed) return onError(new Error(`${h.name}: ${t("fees.pay.amount")}`));
      lines.push({ fee_head_id: h.id, amount: parsed });
    }
    setBusy(true);
    try {
      await api.setFeeStructure({ academic_year_id: selectedYear, class_id: selectedClass, lines });
      await structure.reload();
      onSaved();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title={t("fees.setup.structure")} className="xl:col-span-2">
      <p className="mb-3 text-xs text-muted">{t("fees.setup.structure.hint")}</p>
      <div className="mb-3 flex gap-2">
        <Select
          label={t("academics.sections.year")}
          value={selectedYear}
          onChange={(e) => setYearId(e.target.value)}
          options={(years.data ?? []).map((y) => ({ value: y.id, label: y.name }))}
          className="w-40"
        />
        <Select
          label={t("common.class")}
          value={selectedClass}
          onChange={(e) => setClassId(e.target.value)}
          options={(classes.data ?? []).map((c) => ({ value: c.id, label: c.name }))}
          className="w-32"
        />
      </div>
      {heads.length === 0 ? (
        <p className="text-sm text-muted">{t("fees.setup.structure.noHeads")}</p>
      ) : (
        <form onSubmit={save} noValidate className="flex flex-col gap-2">
          <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
            {heads.map((h) => (
              <Input
                key={h.id}
                label={`${h.name} ${h.is_recurring ? "" : `(${t("fees.setup.heads.oneTime")})`}`}
                inputMode="decimal"
                value={amounts[h.id] ?? "0"}
                onChange={(e) => setAmounts({ ...amounts, [h.id]: e.target.value })}
                disabled={!canEdit}
              />
            ))}
          </div>
          {canEdit && (
            <div className="flex justify-end">
              <Button type="submit" loading={busy} disabled={!selectedYear || !selectedClass}>
                {t("action.save")}
              </Button>
            </div>
          )}
        </form>
      )}
    </Card>
  );
}
