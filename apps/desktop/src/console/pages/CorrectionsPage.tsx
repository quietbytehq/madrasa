import { useState } from "react";
import type { FormEvent } from "react";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n";
import { useAsync } from "@/lib/useAsync";
import { hub } from "../api";
import type { CorrectionKind, CorrectionRecord, FoundRecords } from "../api";
import { useFail } from "../session";
import { td, th } from "../format";
import { PageTitle } from "./common";

const KIND_LABEL: Record<string, TranslationKey> = {
  school: "hub.corrections.kind.school",
  students: "hub.corrections.kind.students",
  guardians: "hub.corrections.kind.guardians",
  staff: "hub.corrections.kind.staff",
};
const FIELD_LABEL: Record<string, TranslationKey> = {
  name: "hub.corrections.field.name",
  city: "hub.corrections.field.city",
  full_name: "hub.corrections.field.full_name",
  father_name: "hub.corrections.field.father_name",
  address: "hub.corrections.field.address",
  phone: "hub.corrections.field.phone",
  email: "hub.corrections.field.email",
};

/**
 * Head office corrections: find a record in a school's copy and correct a name or a
 * contact detail. The school receives the change at its next sync.
 */
export function CorrectionsPage() {
  const { t } = useI18n();
  const fail = useFail();
  const toast = useToast();
  const schools = useAsync(() => hub.schools(), []);
  const kinds = useAsync(() => hub.correctionKinds(), []);
  const [school, setSchool] = useState("");
  const [kindCode, setKindCode] = useState("students");
  const [q, setQ] = useState("");
  const [found, setFound] = useState<FoundRecords | null>(null);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<CorrectionRecord | null>(null);

  const kind = kinds.data?.find((k) => k.code === kindCode) ?? null;
  const label = (map: Record<string, TranslationKey>, key: string) =>
    map[key] ? t(map[key]) : key;

  const search = async (page = 1) => {
    if (!school) return;
    setBusy(true);
    try {
      setFound(await hub.records(school, kindCode, q.trim(), page));
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    void search();
  };

  return (
    <>
      <PageTitle title={t("hub.nav.corrections")} about={t("hub.corrections.about")} />
      <Card>
        <form onSubmit={submit} className="grid items-end gap-3 md:grid-cols-[1fr_1fr_1fr_auto]">
          <Select
            label={t("hub.schools.school")}
            value={school}
            onChange={(e) => {
              setSchool(e.target.value);
              setFound(null);
            }}
            placeholder={t("hub.corrections.chooseSchool")}
            options={(schools.data ?? [])
              .filter((s) => s.status === "active")
              .map((s) => ({ value: s.id, label: `${s.name} (${s.code})` }))}
          />
          <Select
            label={t("hub.corrections.kind")}
            value={kindCode}
            onChange={(e) => {
              setKindCode(e.target.value);
              setFound(null);
            }}
            options={(kinds.data ?? []).map((k) => ({
              value: k.code,
              label: label(KIND_LABEL, k.code),
            }))}
          />
          <Input
            label={t("hub.corrections.search")}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            disabled={kindCode === "school"}
          />
          <Button type="submit" loading={busy} disabled={!school}>
            {t("action.search")}
          </Button>
        </form>
      </Card>

      {found && kind && (
        <Card className="mt-4" padded={false}>
          {found.items.length === 0 ? (
            <p className="p-4 text-sm text-muted">{t("hub.corrections.none")}</p>
          ) : (
            <>
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead className="border-b border-border">
                    <tr>
                      {kind.fields.map((f) => (
                        <th key={f.name} className={th} scope="col">
                          {label(FIELD_LABEL, f.name)}
                        </th>
                      ))}
                      <th className={th} />
                    </tr>
                  </thead>
                  <tbody>
                    {found.items.map((r) => (
                      <tr key={r.id} className="border-t border-border">
                        {kind.fields.map((f, i) => (
                          <td key={f.name} className={td}>
                            {r.fields[f.name] || "—"}
                            {i === 0 && r.hint && (
                              <div className="text-xs text-muted" dir="ltr">
                                {r.hint}
                              </div>
                            )}
                          </td>
                        ))}
                        <td className={`${td} text-end`}>
                          <Button variant="secondary" size="sm" onClick={() => setEditing(r)}>
                            {t("hub.corrections.correct")}
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="flex items-center justify-between gap-2 border-t border-border px-3 py-2 text-xs text-muted">
                <span>{t("hub.corrections.count", { count: found.total })}</span>
                <span className="flex gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={found.page <= 1}
                    onClick={() => void search(found.page - 1)}
                  >
                    {t("action.back")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={found.page * found.page_size >= found.total}
                    onClick={() => void search(found.page + 1)}
                  >
                    {t("action.continue")}
                  </Button>
                </span>
              </div>
            </>
          )}
        </Card>
      )}

      {editing && kind && (
        <CorrectDialog
          school={school}
          kind={kind}
          record={editing}
          fieldLabel={(name) => label(FIELD_LABEL, name)}
          onClose={() => setEditing(null)}
          onSaved={(changed) => {
            setEditing(null);
            toast.push({
              kind: "success",
              title: changed ? t("hub.corrections.saved") : t("hub.corrections.unchanged"),
            });
            void search(found?.page ?? 1);
          }}
        />
      )}
    </>
  );
}

function CorrectDialog({
  school,
  kind,
  record,
  fieldLabel,
  onClose,
  onSaved,
}: {
  school: string;
  kind: CorrectionKind;
  record: CorrectionRecord;
  fieldLabel: (name: string) => string;
  onClose: () => void;
  onSaved: (changed: boolean) => void;
}) {
  const { t } = useI18n();
  const fail = useFail();
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(kind.fields.map((f) => [f.name, record.fields[f.name] ?? ""])),
  );
  const [busy, setBusy] = useState(false);
  // Only what was really changed is sent, so untouched fields cannot collide with the school.
  const changed = Object.fromEntries(
    kind.fields
      .filter((f) => (values[f.name] ?? "").trim() !== (record.fields[f.name] ?? ""))
      .map((f) => [f.name, (values[f.name] ?? "").trim()]),
  );
  const missing = kind.fields.some((f) => f.required && !(values[f.name] ?? "").trim());

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const done = await hub.correct(school, kind.code, record.id, changed);
      onSaved(done.changed.length > 0);
    } catch (err) {
      fail(err);
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={t("hub.corrections.title", { name: record.title })}
      description={t("hub.corrections.rule")}
    >
      <form onSubmit={(e) => void submit(e)} noValidate className="flex flex-col gap-3">
        {kind.fields.map((f, i) => (
          <Input
            key={f.name}
            label={fieldLabel(f.name)}
            labelSuffix={f.required ? undefined : t("field.optional")}
            value={values[f.name] ?? ""}
            onChange={(e) => setValues((v) => ({ ...v, [f.name]: e.target.value }))}
            autoFocus={i === 0}
            dir={f.name === "email" || f.name === "phone" ? "ltr" : undefined}
          />
        ))}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            type="submit"
            loading={busy}
            disabled={missing || Object.keys(changed).length === 0}
          >
            {t("hub.corrections.save")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
