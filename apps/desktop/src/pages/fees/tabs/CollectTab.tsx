import { useState } from "react";
import { api } from "@/api/endpoints";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { LedgerView } from "../LedgerView";

/** Find a student, then work in their ledger (pay, waive, reverse). */
export function CollectTab() {
  const { t } = useI18n();
  const [q, setQ] = useState("");
  const [studentId, setStudentId] = useState<string | null>(null);
  const results = useAsync(
    () =>
      q.trim().length >= 2 ? api.students({ q: q.trim(), page_size: 10 }) : Promise.resolve(null),
    [q],
  );
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-end gap-2">
        <Input
          label={t("fees.student.search")}
          type="search"
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setStudentId(null);
          }}
          className="w-96"
          autoFocus
        />
      </div>
      {!studentId && results.data && (
        <Card padded={false}>
          {results.data.items.length === 0 ? (
            <EmptyState title={t("students.empty")} />
          ) : (
            <ul className="divide-y divide-border text-sm">
              {results.data.items.map((s) => (
                <li key={s.id}>
                  <button
                    type="button"
                    onClick={() => setStudentId(s.id)}
                    className="flex w-full items-center gap-3 px-4 py-2 text-start hover:bg-selection"
                  >
                    <span className="font-mono text-xs text-muted">{s.student_number}</span>
                    <span className="font-medium text-text">{s.full_name}</span>
                    <span className="text-muted">{s.father_name}</span>
                    <span className="flex-1" />
                    <span className="text-xs text-muted">
                      {s.class_name ? `${s.class_name}-${s.section_name ?? ""}` : ""}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}
      {studentId && <LedgerView studentId={studentId} />}
    </div>
  );
}
