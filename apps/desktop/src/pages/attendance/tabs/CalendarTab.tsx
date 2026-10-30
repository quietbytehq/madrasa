import { useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { useSession } from "@/app/session";

const WEEKDAYS = [1, 2, 3, 4, 5, 6, 7] as const;

/** Weekly off days and the holiday list. Editable with academics.manage. */
export function CalendarTab() {
  const { t, formatDate } = useI18n();
  const { can } = useSession();
  const toast = useToast();
  const canEdit = can("academics.manage");
  const cal = useAsync(() => api.calendar(), []);
  // Local edit of the weekend days; null until the user changes something.
  const [weekendEdit, setWeekendEdit] = useState<number[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [name, setName] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const fail = (err: unknown) =>
    toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });

  const weekend = weekendEdit ?? cal.data?.weekend_days ?? [];
  const saveWeekend = async () => {
    if (weekend.length >= 7) return fail({ message: t("attendance.calendar.atLeastOneDay") });
    setBusy(true);
    try {
      cal.setData(await api.setWeekend(weekend));
      setWeekendEdit(null);
      toast.push({ kind: "success", title: t("common.saved") });
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const addHoliday = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !from) return;
    setBusy(true);
    try {
      await api.createHoliday({ name: name.trim(), starts_on: from, ends_on: to || undefined });
      setName("");
      setFrom("");
      setTo("");
      await cal.reload();
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };

  if (cal.loading && !cal.data)
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  if (cal.error || !cal.data)
    return <EmptyState title={errorMessage(cal.error, t("common.error.load"))} />;
  return (
    <div className="grid gap-4 xl:grid-cols-2">
      <Card title={t("attendance.calendar.weekend")}>
        <p className="mb-2 text-xs text-muted">{t("attendance.calendar.weekendHint")}</p>
        <div className="flex flex-wrap gap-3">
          {WEEKDAYS.map((d) => (
            <label key={d} className="flex items-center gap-1.5 text-sm">
              <input
                type="checkbox"
                checked={weekend.includes(d)}
                disabled={!canEdit}
                onChange={(e) =>
                  setWeekendEdit(
                    e.target.checked
                      ? [...weekend, d].sort((a, b) => a - b)
                      : weekend.filter((x) => x !== d),
                  )
                }
              />
              {t(`weekday.${d}`)}
            </label>
          ))}
        </div>
        {canEdit && (
          <div className="mt-3 flex justify-end">
            <Button loading={busy} onClick={() => void saveWeekend()}>
              {t("action.save")}
            </Button>
          </div>
        )}
      </Card>
      <Card title={t("attendance.calendar.holidays")} padded={false}>
        {canEdit && (
          <form
            onSubmit={addHoliday}
            noValidate
            className="flex flex-wrap items-end gap-2 border-b border-border p-3"
          >
            <Input
              label={t("attendance.calendar.name")}
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-44"
              required
            />
            <Input
              label={t("attendance.calendar.from")}
              type="date"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              className="w-40"
              required
            />
            <Input
              label={t("attendance.calendar.to")}
              labelSuffix={t("field.optional")}
              type="date"
              value={to}
              min={from || undefined}
              onChange={(e) => setTo(e.target.value)}
              className="w-40"
            />
            <Button type="submit" loading={busy} disabled={!name.trim() || !from}>
              {t("attendance.calendar.addHoliday")}
            </Button>
          </form>
        )}
        {cal.data.holidays.length === 0 ? (
          <EmptyState title={t("attendance.calendar.empty")} />
        ) : (
          <ul className="divide-y divide-border text-sm">
            {cal.data.holidays.map((h) => (
              <li key={h.id} className="flex items-center gap-3 px-4 py-2">
                <span className="font-medium">{h.name}</span>
                <span className="text-muted">
                  {formatDate(h.starts_on)}
                  {h.ends_on !== h.starts_on ? ` – ${formatDate(h.ends_on)}` : ""}
                </span>
                <span className="flex-1" />
                {canEdit && (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-danger"
                    onClick={() =>
                      void api
                        .deleteHoliday(h.id)
                        .then(() => cal.reload())
                        .catch(fail)
                    }
                  >
                    {t("attendance.calendar.remove")}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}
