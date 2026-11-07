import { useEffect, useRef, useState } from "react";
import { api } from "@/api/endpoints";
import type { Timetable, TimetableJob, TimetableSetup } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Table } from "@/components/ui/Table";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/useAsync";
import { CheckCard } from "./LessonsTab";
import { Hint, StatusBadge } from "./common";

interface Props {
  setup: TimetableSetup;
  manage: boolean;
  reload: () => Promise<void>;
  onOpen: (timetableId: string) => void;
}

/** Generation: run the solver in the background, follow its progress, manage timetables. */
export function GenerateTab({ setup, manage, reload, onOpen }: Props) {
  const { t, formatDate } = useI18n();
  const toast = useToast();
  const [name, setName] = useState("");
  const [into, setInto] = useState("");
  const [limit, setLimit] = useState(60);
  const [improve, setImprove] = useState<number | "">("");
  const [job, setJob] = useState<TimetableJob | null>(null);
  const [busy, setBusy] = useState(false);
  const timer = useRef<number | null>(null);

  // Poll a running job until it finishes.
  useEffect(() => {
    if (!job || job.status !== "running") return;
    timer.current = window.setTimeout(async () => {
      try {
        const next = await api.timetableJob(job.id);
        setJob(next);
        if (next.status !== "running") await reload();
      } catch (err) {
        toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
        setJob(null);
      }
    }, 1000);
    return () => {
      if (timer.current) window.clearTimeout(timer.current);
    };
  }, [job, reload, t, toast]);

  const generate = async () => {
    setBusy(true);
    try {
      const started = await api.generateTimetable({
        academic_year_id: setup.academic_year.id,
        timetable_id: into || undefined,
        name: name || undefined,
        time_limit_seconds: limit,
        improve_seconds: improve === "" ? undefined : improve,
      });
      setJob(started);
      await reload();
    } catch (err) {
      toast.push({
        kind: "error",
        title: t("tt.generate.blocked", { message: errorMessage(err, t("app.error.generic")) }),
      });
    } finally {
      setBusy(false);
    }
  };
  const act = async (f: () => Promise<unknown>, done?: string) => {
    try {
      await f();
      await reload();
      if (done) toast.push({ kind: "success", title: done });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    }
  };
  const report = job?.report ?? null;
  return (
    <div className="flex flex-col gap-4">
      <CheckCard setup={setup} />
      {manage && (
        <Card title={t("tt.generate.run")}>
          <Hint>{t("tt.generate.hint")}</Hint>
          <div className="flex flex-wrap items-end gap-3">
            <Select
              label={t("tt.generate.into")}
              value={into}
              onChange={(e) => setInto(e.target.value)}
              options={[
                { value: "", label: t("tt.generate.new") },
                ...setup.timetables.map((x) => ({ value: x.id, label: x.name })),
              ]}
              className="w-52"
            />
            {!into && (
              <Input
                label={t("tt.generate.name")}
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="w-48"
              />
            )}
            <Input
              label={t("tt.generate.timeLimit")}
              type="number"
              min={1}
              max={600}
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value) || 60)}
              className="w-28"
            />
            <Input
              label={t("tt.generate.improve")}
              type="number"
              min={0}
              max={300}
              value={improve}
              placeholder={String(setup.school_rules.improve_seconds)}
              onChange={(e) => setImprove(e.target.value === "" ? "" : Number(e.target.value))}
              className="w-28"
            />
            {job?.status === "running" ? (
              <Button variant="secondary" onClick={() => void api.cancelTimetableJob(job.id)}>
                {t("tt.generate.cancel")}
              </Button>
            ) : (
              <Button
                loading={busy}
                disabled={setup.check.errors > 0}
                onClick={() => void generate()}
                data-testid="generate"
              >
                {t("tt.generate.run")}
              </Button>
            )}
          </div>
          {job && (
            <div className="mt-3 text-sm" data-testid="job-status">
              {job.status === "running" && (
                <div>
                  <div className="flex items-center gap-2">
                    <span>{t("tt.generate.running")}</span>
                    <span className="text-muted">
                      {t(
                        job.phase === "improving"
                          ? "tt.generate.phase.improving"
                          : "tt.generate.phase.placing",
                      )}
                      {" · "}
                      {t("tt.generate.progress", { placed: job.placed, total: job.total })}
                    </span>
                  </div>
                  <div className="bg-surface mt-1 h-2 w-full overflow-hidden rounded">
                    <div
                      className="h-2 bg-blue transition-all"
                      style={{
                        width: `${Math.round(
                          (job.phase === "improving"
                            ? job.fraction
                            : job.total
                              ? job.placed / job.total
                              : 0) * 100,
                        )}%`,
                      }}
                    />
                  </div>
                </div>
              )}
              {job.status === "failed" && (
                <Badge tone="danger">{job.error ?? t("tt.generate.failed")}</Badge>
              )}
              {job.status === "done" && report && (
                <div className="flex flex-col gap-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge tone={report.placed === report.total ? "success" : "warning"}>
                      {t("tt.generate.done", {
                        placed: report.placed,
                        total: report.total,
                        seconds: Math.round(report.seconds),
                      })}
                    </Badge>
                    <Button size="sm" variant="secondary" onClick={() => onOpen(job.timetable_id)}>
                      {t("tt.timetable.open")}
                    </Button>
                  </div>
                  {report.unplaced.length > 0 && (
                    <div>
                      <div className="text-xs font-medium text-muted">
                        {t("tt.generate.unplaced")}
                      </div>
                      <ul className="mt-1 flex flex-col gap-1 text-xs">
                        {report.unplaced.map((u, i) => (
                          <li key={i}>
                            <span className="font-medium">{u.title}</span>: {u.message}{" "}
                            <span className="text-muted">{u.fix}</span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                  {report.relaxed.length > 0 && (
                    <div>
                      <div className="text-xs font-medium text-muted">
                        {t("tt.generate.relaxed")}
                      </div>
                      <ul className="mt-1 flex flex-col gap-1 text-xs">
                        {report.relaxed.map((r, i) => (
                          <li key={i}>{r.message}</li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </Card>
      )}
      <Card title={t("tt.timetables")}>
        {setup.timetables.length === 0 ? (
          <EmptyState title={t("tt.timetables.empty")} />
        ) : (
          <Table<Timetable>
            rows={setup.timetables}
            rowKey={(x) => x.id}
            columns={[
              { key: "name", header: t("tt.timetable.name"), render: (x) => x.name },
              {
                key: "status",
                header: t("tt.timetable.status"),
                render: (x) => <StatusBadge status={x.status} />,
              },
              {
                key: "generated",
                header: t("tt.timetable.generated"),
                render: (x) =>
                  x.generated_at
                    ? `${formatDate(x.generated_at)} · ${x.report?.placed ?? "?"}/${x.report?.total ?? "?"}`
                    : "—",
              },
              {
                key: "actions",
                header: "",
                render: (x) => (
                  <div className="flex flex-wrap justify-end gap-1">
                    <Button size="sm" variant="ghost" onClick={() => onOpen(x.id)}>
                      {t("tt.timetable.open")}
                    </Button>
                    {manage && x.status !== "published" && (
                      <>
                        <Button
                          size="sm"
                          variant="secondary"
                          onClick={() => {
                            if (window.confirm(t("tt.timetable.publishConfirm")))
                              void act(
                                () => api.publishTimetable(x.id),
                                t("tt.timetable.published.toast"),
                              );
                          }}
                        >
                          {t("tt.timetable.publish")}
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => {
                            if (window.confirm(t("tt.timetable.deleteConfirm")))
                              void act(() => api.deleteTimetable(x.id));
                          }}
                        >
                          {t("tt.timetable.delete")}
                        </Button>
                      </>
                    )}
                    {manage && (
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                          const next = window.prompt(t("tt.timetable.rename"), x.name);
                          if (next && next.trim())
                            void act(() => api.renameTimetable(x.id, next.trim()));
                        }}
                      >
                        {t("tt.timetable.rename")}
                      </Button>
                    )}
                  </div>
                ),
              },
            ]}
            pageSize={0}
          />
        )}
      </Card>
    </div>
  );
}
