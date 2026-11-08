import { useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { api } from "@/api/endpoints";
import type { AdmissionsBoard } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useSession } from "@/app/session";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { PageHeader } from "../PageHeader";
import { ApplicationDialog } from "./ApplicationDialog";
import { BoardTab } from "./BoardTab";
import { ListTab } from "./ListTab";
import { StagesTab } from "./StagesTab";

const TABS: { key: string; labelKey: TranslationKey; manage?: boolean }[] = [
  { key: "board", labelKey: "adm.tab.board" },
  { key: "list", labelKey: "adm.tab.list" },
  { key: "stages", labelKey: "adm.tab.stages", manage: true },
];

/** The Admissions module (Standard stage 3). */
export function AdmissionsPage() {
  const { t } = useI18n();
  const { can } = useSession();
  const manage = can("admissions.manage");
  const [params, setParams] = useSearchParams();
  const [tab, setTab] = useState(params.get("tab") ?? "board");
  const [query, setQuery] = useState("");
  const [classId, setClassId] = useState("");
  const board = useAsync(
    () => api.admissionsBoard({ q: query || undefined, class_id: classId || undefined }),
    [query, classId],
  );
  const [open, setOpen] = useState<string | "new" | null>(params.get("open"));
  const select = (key: string) => {
    setTab(key);
    const next = new URLSearchParams(params);
    next.set("tab", key);
    next.delete("open");
    setParams(next, { replace: true });
  };
  const data: AdmissionsBoard | null = board.data;
  return (
    <>
      <PageHeader
        title={t("adm.title")}
        crumbs={[{ label: t("nav.dashboard"), to: "/" }, { label: t("adm.title") }]}
        actions={
          manage && (
            <div className="flex gap-2">
              <Link to="/import?kind=enquiries">
                <Button size="sm" variant="secondary">
                  {t("adm.import")}
                </Button>
              </Link>
              <Button size="sm" onClick={() => setOpen("new")} data-testid="new-application">
                {t("adm.new")}
              </Button>
            </div>
          )
        }
      />
      <div
        role="tablist"
        aria-label={t("adm.title")}
        className="mb-3 flex gap-1 border-b border-border"
      >
        {TABS.filter((tb) => manage || !tb.manage).map((tb) => (
          <button
            key={tb.key}
            role="tab"
            type="button"
            aria-selected={tab === tb.key}
            onClick={() => select(tb.key)}
            className={`-mb-px border-b-2 px-3 py-2 text-sm ${
              tab === tb.key
                ? "border-blue font-medium text-blue"
                : "border-transparent text-muted hover:text-text"
            }`}
          >
            {t(tb.labelKey)}
          </button>
        ))}
      </div>
      {tab === "stages" && manage ? (
        <StagesTab onChanged={board.reload} />
      ) : board.loading && !data ? (
        <div className="flex justify-center py-10">
          <Spinner size="lg" />
        </div>
      ) : board.error || !data ? (
        <Card>
          <EmptyState title={errorMessage(board.error, t("tt.noYear"))} />
        </Card>
      ) : tab === "list" ? (
        <ListTab
          board={data}
          manage={manage}
          query={query}
          classId={classId}
          onQuery={setQuery}
          onClass={setClassId}
          onOpen={setOpen}
          reload={board.reload}
        />
      ) : (
        <BoardTab board={data} manage={manage} onOpen={setOpen} reload={board.reload} />
      )}
      {open && data && (
        <ApplicationDialog
          id={open === "new" ? null : open}
          board={data}
          manage={manage}
          onClose={() => setOpen(null)}
          onChanged={() => void board.reload()}
        />
      )}
    </>
  );
}
