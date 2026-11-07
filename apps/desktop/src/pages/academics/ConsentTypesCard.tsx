import { useState } from "react";
import type { FormEvent } from "react";
import { api } from "@/api/endpoints";
import type { ConsentGivenBy, ConsentRenewal, ConsentRequired, ConsentType } from "@/api/types";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useI18n } from "@/i18n";
import { useAsync } from "@/lib/useAsync";

type Draft = {
  id: string | null;
  name: string;
  description: string;
  given_by: ConsentGivenBy;
  required: ConsentRequired;
  min_level: string;
  max_level: string;
  renewal: ConsentRenewal;
};

const EMPTY: Draft = {
  id: null,
  name: "",
  description: "",
  given_by: "guardian",
  required: "all",
  min_level: "",
  max_level: "",
  renewal: "none",
};

/** Consent types defined by the school (M2 stage 8). */
export function ConsentTypesCard({
  manage,
  onError,
}: {
  manage: boolean;
  onError: (err: unknown) => void;
}) {
  const { t } = useI18n();
  const types = useAsync(() => api.consentTypes(), []);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [busy, setBusy] = useState(false);
  const edit = (x: ConsentType) =>
    setDraft({
      id: x.id,
      name: x.name,
      description: x.description,
      given_by: x.given_by,
      required: x.required,
      min_level: x.min_level === null ? "" : String(x.min_level),
      max_level: x.max_level === null ? "" : String(x.max_level),
      renewal: x.renewal,
    });
  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!draft || !draft.name.trim()) return;
    setBusy(true);
    try {
      const body = {
        name: draft.name.trim(),
        description: draft.description.trim(),
        given_by: draft.given_by,
        required: draft.required,
        min_level: draft.min_level === "" ? null : Number(draft.min_level),
        max_level: draft.max_level === "" ? null : Number(draft.max_level),
        renewal: draft.renewal,
        position: draft.id
          ? (types.data?.find((x) => x.id === draft.id)?.position ?? 0)
          : (types.data?.length ?? 0),
      };
      if (draft.id) await api.updateConsentType(draft.id, body);
      else await api.createConsentType(body);
      setDraft(null);
      await types.reload();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  const remove = async (id: string) => {
    setBusy(true);
    try {
      await api.deleteConsentType(id);
      await types.reload();
    } catch (err) {
      onError(err);
    } finally {
      setBusy(false);
    }
  };
  const requiredLabel = (x: ConsentType) =>
    x.required === "class_range"
      ? `${t("consents.types.required.class_range")} ${x.min_level ?? ""}–${x.max_level ?? ""}`
      : t(`consents.types.required.${x.required}`);
  return (
    <Card
      title={t("consents.types")}
      padded={false}
      actions={
        manage &&
        !draft && (
          <Button size="sm" variant="secondary" onClick={() => setDraft(EMPTY)}>
            {t("consents.types.new")}
          </Button>
        )
      }
    >
      <p className="px-4 pt-3 text-xs text-muted">{t("consents.types.hint")}</p>
      {types.loading && !types.data ? (
        <div className="flex justify-center p-4">
          <Spinner />
        </div>
      ) : (
        <table className="w-full text-sm" aria-label={t("consents.types")}>
          <thead className="bg-hover text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-4 py-2 text-start">{t("consents.types.name")}</th>
              <th className="px-4 py-2 text-start">{t("consents.types.givenBy")}</th>
              <th className="px-4 py-2 text-start">{t("consents.types.required")}</th>
              <th className="px-4 py-2 text-start">{t("consents.types.renewal")}</th>
              <th className="w-32 px-4 py-2" />
            </tr>
          </thead>
          <tbody>
            {(types.data ?? []).length === 0 && (
              <tr>
                <td colSpan={5} className="px-4 py-3 text-muted">
                  {t("consents.types.empty")}
                </td>
              </tr>
            )}
            {(types.data ?? []).map((x) => (
              <tr key={x.id} className="border-t border-border">
                <td className="px-4 py-2">
                  <div className="font-medium">{x.name}</div>
                  {x.description && <div className="text-xs text-muted">{x.description}</div>}
                </td>
                <td className="px-4 py-2">{t(`consents.types.givenBy.${x.given_by}`)}</td>
                <td className="px-4 py-2">{requiredLabel(x)}</td>
                <td className="px-4 py-2">{t(`consents.types.renewal.${x.renewal}`)}</td>
                <td className="px-4 py-2 text-end">
                  {manage && (
                    <div className="flex justify-end gap-1">
                      <Button size="sm" variant="ghost" onClick={() => edit(x)}>
                        {t("action.edit")}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        loading={busy}
                        onClick={() => void remove(x.id)}
                      >
                        {t("action.delete")}
                      </Button>
                    </div>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {draft && (
        <form
          onSubmit={save}
          className="flex flex-col gap-2 border-t border-border p-4"
          data-testid="consent-type-editor"
        >
          <div className="flex flex-wrap items-end gap-2">
            <Input
              label={t("consents.types.name")}
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              className="w-56"
              required
            />
            <Input
              label={t("consents.types.description")}
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
              className="w-80"
            />
          </div>
          <div className="flex flex-wrap items-end gap-2">
            <Select
              label={t("consents.types.givenBy")}
              value={draft.given_by}
              onChange={(e) => setDraft({ ...draft, given_by: e.target.value as ConsentGivenBy })}
              options={(["guardian", "student", "either"] as const).map((v) => ({
                value: v,
                label: t(`consents.types.givenBy.${v}`),
              }))}
              className="w-48"
            />
            <Select
              label={t("consents.types.required")}
              value={draft.required}
              onChange={(e) => setDraft({ ...draft, required: e.target.value as ConsentRequired })}
              options={(["all", "class_range", "none"] as const).map((v) => ({
                value: v,
                label: t(`consents.types.required.${v}`),
              }))}
              className="w-44"
            />
            {draft.required === "class_range" && (
              <>
                <Input
                  label={t("consents.types.minLevel")}
                  type="number"
                  value={draft.min_level}
                  onChange={(e) => setDraft({ ...draft, min_level: e.target.value })}
                  className="w-24"
                />
                <Input
                  label={t("consents.types.maxLevel")}
                  type="number"
                  value={draft.max_level}
                  onChange={(e) => setDraft({ ...draft, max_level: e.target.value })}
                  className="w-24"
                />
              </>
            )}
            <Select
              label={t("consents.types.renewal")}
              value={draft.renewal}
              onChange={(e) => setDraft({ ...draft, renewal: e.target.value as ConsentRenewal })}
              options={(["none", "yearly"] as const).map((v) => ({
                value: v,
                label: t(`consents.types.renewal.${v}`),
              }))}
              className="w-36"
            />
          </div>
          <div className="flex gap-2">
            <Button type="submit" size="sm" loading={busy} disabled={!draft.name.trim()}>
              {t("action.save")}
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setDraft(null)}>
              {t("action.cancel")}
            </Button>
          </div>
        </form>
      )}
    </Card>
  );
}
