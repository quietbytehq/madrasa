import { useState } from "react";
import { Download } from "lucide-react";
import { api } from "@/api/endpoints";
import { Button } from "@/components/ui/Button";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/useAsync";

/** Two small buttons that download the current list as .xlsx or .csv. */
export function ExportMenu({
  path,
  params,
}: {
  path: string;
  params: Record<string, string | number | undefined>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [busy, setBusy] = useState<"xlsx" | "csv" | null>(null);
  const run = async (format: "xlsx" | "csv") => {
    setBusy(format);
    try {
      await api.exportList(path, { ...params, format });
      toast.push({ kind: "success", title: t("export.started") });
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(null);
    }
  };
  return (
    <div className="flex gap-1">
      <Button
        size="sm"
        variant="secondary"
        loading={busy === "xlsx"}
        icon={<Download className="h-3.5 w-3.5" aria-hidden />}
        onClick={() => void run("xlsx")}
      >
        {t("export.excel")}
      </Button>
      <Button size="sm" variant="ghost" loading={busy === "csv"} onClick={() => void run("csv")}>
        {t("export.csv")}
      </Button>
    </div>
  );
}
