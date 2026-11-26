import { Link } from "react-router-dom";
import { CheckCircle2, CircleAlert, RefreshCw, TriangleAlert } from "lucide-react";
import { api } from "@/api/endpoints";
import type { HealthCheck } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";

const AREAS: HealthCheck["area"][] = ["students", "guardians", "fees", "classes"];

/**
 * Data health: what in the school's records needs a person's attention, most often after
 * an import. Each check says what is wrong, why it matters, how many records it found, and
 * links to the first of them. Checks that found nothing are listed too, so the page also
 * says what is in order.
 */
export function DataHealthPage() {
  const { t } = useI18n();
  const health = useAsync(() => api.dataHealth(), []);
  const checks = health.data ?? [];
  const problems = checks.filter((c) => c.count > 0 && c.severity === "problem").length;
  const warnings = checks.filter((c) => c.count > 0 && c.severity === "warning").length;

  return (
    <>
      <PageHeader
        title={t("health.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("health.title") }]}
        actions={
          <Button
            variant="secondary"
            loading={health.loading}
            icon={<RefreshCw className="h-4 w-4" aria-hidden />}
            onClick={() => void health.reload()}
          >
            {t("health.recheck")}
          </Button>
        }
      />
      <p className="mb-3 max-w-3xl text-sm text-muted">{t("health.subtitle")}</p>
      {health.loading && !health.data ? (
        <div className="flex justify-center py-10">
          <Spinner size="lg" />
        </div>
      ) : health.error || !health.data ? (
        <EmptyState title={errorMessage(health.error, t("common.error.load"))} />
      ) : (
        <>
          <p
            role="status"
            data-testid="health-summary"
            className="mb-4 flex items-center gap-2 rounded border border-border bg-card px-3 py-2 text-sm"
          >
            {problems + warnings === 0 ? (
              <>
                <CheckCircle2 className="h-4 w-4 text-success" aria-hidden />
                {t("health.allClear")}
              </>
            ) : (
              <>
                <CircleAlert className="h-4 w-4 text-danger" aria-hidden />
                {t("health.summary", { problems, warnings })}
              </>
            )}
          </p>
          <div className="flex flex-col gap-4">
            {AREAS.map((area) => {
              const list = checks.filter((c) => c.area === area);
              if (list.length === 0) return null;
              return (
                <Card key={area} title={t(`health.area.${area}`)} padded={false}>
                  <ul>
                    {list.map((c) => (
                      <CheckRow key={c.code} check={c} />
                    ))}
                  </ul>
                </Card>
              );
            })}
          </div>
        </>
      )}
    </>
  );
}

function CheckRow({ check }: { check: HealthCheck }) {
  const { t } = useI18n();
  const found = check.count > 0;
  return (
    <li
      className="border-t border-border px-4 py-3 first:border-t-0"
      data-testid={`health-${check.code}`}
    >
      <div className="flex flex-wrap items-center gap-2">
        {!found ? (
          <CheckCircle2 className="h-4 w-4 shrink-0 text-success" aria-hidden />
        ) : check.severity === "problem" ? (
          <CircleAlert className="h-4 w-4 shrink-0 text-danger" aria-hidden />
        ) : (
          <TriangleAlert className="h-4 w-4 shrink-0 text-warning" aria-hidden />
        )}
        <h3 className="text-sm font-medium text-text">{t(`health.check.${check.code}`)}</h3>
        <span className="flex-1" />
        {found ? (
          <Badge tone={check.severity === "problem" ? "danger" : "warning"}>
            <span data-testid={`health-count-${check.code}`}>{check.count}</span>
          </Badge>
        ) : (
          <span className="text-xs text-muted">{t("health.none")}</span>
        )}
      </div>
      {found && (
        <>
          <p className="mt-1 text-xs text-muted">{t(`health.why.${check.code}`)}</p>
          <ul className="mt-2 grid gap-x-6 gap-y-1 text-xs md:grid-cols-2">
            {check.sample.map((f, i) => (
              <li key={i} className="flex gap-2">
                {f.link ? (
                  <Link className="font-medium text-blue hover:underline" to={f.link}>
                    {f.label}
                  </Link>
                ) : (
                  <span className="font-medium text-text">{f.label}</span>
                )}
                <span className="text-muted">{f.detail}</span>
              </li>
            ))}
          </ul>
          {check.count > check.sample.length && (
            <p className="mt-1 text-xs text-muted">
              {t("health.more", { count: check.count - check.sample.length })}
            </p>
          )}
        </>
      )}
    </li>
  );
}
