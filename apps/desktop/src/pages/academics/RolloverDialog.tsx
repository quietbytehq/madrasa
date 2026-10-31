import { useState } from "react";
import { api } from "@/api/endpoints";
import type { RolloverPreview } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { formatMoney } from "@/lib/money";
import { errorMessage, useAsync } from "@/lib/useAsync";

/** Preview and confirm the academic year rollover. Exceptions are typed as student numbers
 * and resolved to ids by the students API before applying. */
export function RolloverDialog({
  onClose,
  onDone,
  onError,
}: {
  onClose: () => void;
  onDone: (message: string) => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  const preview = useAsync(() => api.rolloverPreview(), []);
  const [retain, setRetain] = useState("");
  const [leave, setLeave] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const resolve = async (text: string): Promise<{ ids: string[]; unknown: string[] }> => {
    const numbers = text
      .split(/[\n,;]+/)
      .map((n) => n.trim())
      .filter(Boolean);
    const ids: string[] = [];
    const unknown: string[] = [];
    for (const n of numbers) {
      const page = await api.students({ q: n, page_size: 5 });
      const hit = page.items.find((s) => s.student_number.toUpperCase() === n.toUpperCase());
      if (hit) ids.push(hit.id);
      else unknown.push(n);
    }
    return { ids, unknown };
  };

  const confirm = async () => {
    setBusy(true);
    setProblem(null);
    try {
      const r = await resolve(retain);
      const g = await resolve(leave);
      const unknown = [...r.unknown, ...g.unknown];
      if (unknown.length > 0) {
        setProblem(t("rollover.unknownNumbers", { numbers: unknown.join(", ") }));
        return;
      }
      const report = await api.rollover({ retain: r.ids, graduate: g.ids });
      onDone(
        t("rollover.done", {
          name: report.new_year.name,
          promoted: report.promoted,
          retained: report.retained,
          graduated: report.graduated,
          sections: report.sections_created,
        }),
      );
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };

  const p: RolloverPreview | null = preview.data;
  return (
    <Dialog open onClose={onClose} title={t("rollover.title")} size="lg">
      {preview.loading && !p ? (
        <Spinner />
      ) : preview.error || !p ? (
        <p role="alert" className="text-sm text-danger">
          {errorMessage(preview.error, t("app.error.generic"))}
        </p>
      ) : (
        <div className="flex flex-col gap-3 text-sm">
          <p className="text-muted">{t("rollover.intro")}</p>
          <p>
            <span className="text-muted">{t("rollover.newYear")}:</span>{" "}
            <span className="font-semibold">{p.new_year_name}</span> ·{" "}
            {t("rollover.summary", { promoted: p.promoted, graduating: p.graduating })}
          </p>
          {p.months_into_year < 9 && (
            <p
              role="alert"
              className="rounded border border-warning bg-warning-bg px-3 py-2 text-xs text-text"
            >
              {t("rollover.young", { months: p.months_into_year })}
            </p>
          )}
          {p.new_year_exists && (
            <p
              role="alert"
              className="rounded border border-danger bg-danger-bg px-3 py-2 text-xs text-text"
            >
              {t("rollover.exists", { name: p.new_year_name })}
            </p>
          )}
          <table className="w-full text-xs">
            <thead className="bg-hover text-muted">
              <tr>
                <th className="px-2 py-1 text-start">{t("rollover.section")}</th>
                <th className="px-2 py-1 text-end">{t("rollover.students")}</th>
                <th className="px-2 py-1 text-start">{t("rollover.goesTo")}</th>
              </tr>
            </thead>
            <tbody>
              {p.moves.map((m) => (
                <tr key={m.section_id} className="border-t border-border">
                  <td className="px-2 py-1">
                    {m.class_name}-{m.section_name}
                  </td>
                  <td className="px-2 py-1 text-end tabular-nums">{m.students}</td>
                  <td className="px-2 py-1">
                    {m.target_class_name
                      ? `${m.target_class_name}-${m.section_name}`
                      : t("rollover.graduates")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="text-xs text-muted">
            {t("rollover.balances", {
              students: p.students_with_balance,
              amount: formatMoney(p.balance_carried),
            })}
          </p>
          <div className="grid gap-3 md:grid-cols-2">
            <label className="flex flex-col gap-1 text-xs">
              <span>
                {t("rollover.retain")} — {t("rollover.exceptions")}
              </span>
              <textarea
                aria-label={t("rollover.retain")}
                value={retain}
                onChange={(e) => setRetain(e.target.value)}
                rows={3}
                className="rounded border border-border bg-card p-2 font-mono"
              />
            </label>
            <label className="flex flex-col gap-1 text-xs">
              <span>
                {t("rollover.leave")} — {t("rollover.exceptions")}
              </span>
              <textarea
                aria-label={t("rollover.leave")}
                value={leave}
                onChange={(e) => setLeave(e.target.value)}
                rows={3}
                className="rounded border border-border bg-card p-2 font-mono"
              />
            </label>
          </div>
          {problem && (
            <p role="alert" className="text-xs text-danger">
              {problem}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={onClose}>
              {t("action.cancel")}
            </Button>
            <Button
              variant="danger"
              loading={busy}
              disabled={p.new_year_exists}
              onClick={() => void confirm()}
            >
              {t("rollover.confirm")}
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
}
