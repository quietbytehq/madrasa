import { useState } from "react";
import { useSearchParams } from "react-router-dom";
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
  // The student being worked on is part of the address, so that Back from a receipt or a
  // printed voucher returns to their ledger and not to an empty search.
  const [params, setParams] = useSearchParams();
  const studentId = params.get("student");
  const setStudentId = (id: string | null) => {
    const next = new URLSearchParams(params);
    if (id) next.set("student", id);
    else next.delete("student");
    setParams(next, { replace: id === null });
  };
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
            if (studentId) setStudentId(null);
          }}
          className="w-96"
          autoFocus
        />
      </div>
      {!studentId && !results.data && (
        <p className="text-sm text-muted" data-testid="collect-hint">
          {t("fees.collect.hint")}
        </p>
      )}
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
