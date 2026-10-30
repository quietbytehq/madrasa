import { CalendarCheck, Coins, UserRound, Users } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { useI18n } from "@/i18n";
import { useFeatures } from "@/app/features";
import { useSession } from "@/app/session";
import { PageHeader } from "./PageHeader";

export function Dashboard() {
  const { t } = useI18n();
  const { user } = useSession();
  const { edition } = useFeatures();

  const cards = [
    { key: "students", icon: Users, label: t("dashboard.card.students") },
    { key: "attendance", icon: CalendarCheck, label: t("dashboard.card.attendanceToday") },
    { key: "fees", icon: Coins, label: t("dashboard.card.feesCollected") },
    { key: "staff", icon: UserRound, label: t("dashboard.card.staff") },
  ];

  return (
    <>
      <PageHeader
        title={t("dashboard.title")}
        actions={
          <Badge tone="lavender">
            {t("dashboard.edition")}: {t(`edition.${edition}`)}
          </Badge>
        }
      />
      <p className="mb-4 text-sm text-muted">
        {t("dashboard.welcome", { name: user?.full_name ?? "" })}
      </p>
      <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
        {cards.map(({ key, icon: Icon, label }) => (
          <Card key={key} padded={false} className="p-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium uppercase tracking-wide text-muted">
                {label}
              </span>
              <Icon className="h-4 w-4 text-muted" aria-hidden />
            </div>
            <div className="mt-2 text-2xl font-semibold tabular-nums text-navy">—</div>
            <p className="mt-1 text-xs text-muted">{t("dashboard.card.placeholder")}</p>
          </Card>
        ))}
      </div>
      <Card title={t("dashboard.recentActivity")} className="mt-3">
        <EmptyState
          title={t("dashboard.recentActivity")}
          description={t("dashboard.recentActivityEmpty")}
        />
      </Card>
    </>
  );
}
