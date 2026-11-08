import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "@/api/endpoints";
import type { AdmissionsBoard, ApplicationBody, ApplicationDetail } from "@/api/types";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useI18n } from "@/i18n";
import { errorMessage, useAsync } from "@/lib/useAsync";
import { GENDERS, RELATIONSHIPS, genderLabel, relationshipLabel } from "../students/common";
import { StageBadge, today } from "./common";

type Form = Required<Omit<ApplicationBody, "academic_year_id" | "stage_id">> & { stage_id: string };

function blank(classId: string): Form {
  return {
    class_id: classId || null,
    stage_id: "",
    enquiry_date: today(),
    source: "",
    full_name: "",
    father_name: "",
    date_of_birth: null,
    gender: "",
    cnic: null,
    address: "",
    phone: null,
    email: null,
    previous_school: "",
    guardian_name: "",
    guardian_relationship: "father",
    guardian_phone: null,
    guardian_cnic: null,
    guardian_email: null,
    guardian_occupation: "",
    test_date: null,
    test_marks: null,
    test_total: null,
    interview_date: null,
    interview_notes: "",
    offer_date: null,
    offer_fee: null,
    offer_expires: null,
    notes: "",
  };
}

function fromDetail(d: ApplicationDetail): Form {
  const num = (v: string | null) => (v === null ? null : Number(v));
  return {
    class_id: d.class_id,
    stage_id: d.stage_id,
    enquiry_date: d.enquiry_date,
    source: d.source,
    full_name: d.full_name,
    father_name: d.father_name,
    date_of_birth: d.date_of_birth,
    gender: d.gender,
    cnic: d.cnic,
    address: d.address,
    phone: d.phone,
    email: d.email,
    previous_school: d.previous_school,
    guardian_name: d.guardian_name,
    guardian_relationship: d.guardian_relationship,
    guardian_phone: d.guardian_phone,
    guardian_cnic: d.guardian_cnic,
    guardian_email: d.guardian_email,
    guardian_occupation: d.guardian_occupation,
    test_date: d.test_date,
    test_marks: num(d.test_marks),
    test_total: num(d.test_total),
    interview_date: d.interview_date,
    interview_notes: d.interview_notes,
    offer_date: d.offer_date,
    offer_fee: num(d.offer_fee),
    offer_expires: d.offer_expires,
    notes: d.notes,
  };
}

/** One application: the form, its history, stage move, offer letter and conversion. */
export function ApplicationDialog({
  id,
  board,
  manage,
  onClose,
  onChanged,
}: {
  id: string | null;
  board: AdmissionsBoard;
  manage: boolean;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { t, locale, formatDate } = useI18n();
  const toast = useToast();
  const classes = useAsync(() => api.classes(), []);
  const detail = useAsync(() => (id ? api.application(id) : Promise.resolve(null)), [id]);
  const [form, setForm] = useState<Form>(blank(""));
  const [busy, setBusy] = useState(false);
  const [converting, setConverting] = useState(false);
  useEffect(() => {
    if (detail.data) setForm(fromDetail(detail.data));
  }, [detail.data]);
  const d = detail.data;
  const converted = !!d?.student_id;
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setForm((f) => ({ ...f, [k]: v }));
  const text = (k: keyof Form) => ({
    value: (form[k] as string | number | null) ?? "",
    onChange: (e: React.ChangeEvent<HTMLInputElement>) =>
      set(k, (e.target.value === "" ? null : e.target.value) as Form[typeof k]),
  });
  const save = async () => {
    setBusy(true);
    try {
      const body: ApplicationBody = {
        ...form,
        stage_id: id ? undefined : form.stage_id || undefined,
        test_marks: form.test_marks === null ? null : Number(form.test_marks),
        test_total: form.test_total === null ? null : Number(form.test_total),
        offer_fee: form.offer_fee === null ? null : Number(form.offer_fee),
      };
      const saved = id ? await api.updateApplication(id, body) : await api.createApplication(body);
      detail.setData(saved);
      toast.push({ kind: "success", title: t("adm.saved") });
      onChanged();
      if (!id) onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const move = async (stageId: string) => {
    if (!id || !stageId || stageId === d?.stage_id) return;
    setBusy(true);
    try {
      detail.setData(await api.moveApplication(id, stageId));
      onChanged();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!id || !window.confirm(t("adm.deleteConfirm"))) return;
    setBusy(true);
    try {
      await api.deleteApplication(id);
      onChanged();
      onClose();
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  const editable = manage && !converted;
  const admitted = d?.stage_kind === "admitted";
  return (
    <Dialog
      open
      onClose={onClose}
      size="lg"
      title={
        d ? (
          <span className="flex flex-wrap items-center gap-2">
            {d.application_no} · {d.full_name}
            <StageBadge name={d.stage_name} kind={d.stage_kind} />
            {converted && <Badge tone="success">{t("adm.converted")}</Badge>}
          </span>
        ) : (
          t("adm.new")
        )
      }
      footer={
        <div className="flex w-full flex-wrap items-center justify-between gap-2">
          <div className="flex flex-wrap gap-2">
            {id && (
              <>
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => void api.downloadOfferLetter(id, locale)}
                >
                  {t("adm.offerLetter")}
                </Button>
                {locale !== "ur" && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => void api.downloadOfferLetter(id, "ur")}
                  >
                    {t("adm.offerLetterUr")}
                  </Button>
                )}
                {editable && (
                  <Button size="sm" variant="ghost" onClick={() => void remove()} disabled={busy}>
                    {t("action.delete")}
                  </Button>
                )}
              </>
            )}
          </div>
          <div className="flex flex-wrap gap-2">
            {manage && id && !converted && (
              <Button
                size="sm"
                variant="secondary"
                disabled={!admitted}
                title={admitted ? undefined : t("adm.convert.needAdmitted")}
                onClick={() => setConverting(true)}
                data-testid="convert"
              >
                {t("adm.convert")}
              </Button>
            )}
            {converted && d?.student_id && (
              <Link to={`/students/${d.student_id}`}>
                <Button size="sm" variant="secondary">
                  {t("adm.openStudent")} · {d.student_number}
                </Button>
              </Link>
            )}
            <Button variant="secondary" onClick={onClose}>
              {t("action.close")}
            </Button>
            {editable && (
              <Button loading={busy} disabled={!form.full_name.trim()} onClick={() => void save()}>
                {t("action.save")}
              </Button>
            )}
          </div>
        </div>
      }
    >
      {id && detail.loading && !d ? (
        <Spinner />
      ) : (
        <div className="flex flex-col gap-4 text-sm">
          <div className="flex flex-wrap items-end gap-3">
            <Input
              label={t("adm.field.enquiryDate")}
              type="date"
              disabled={!editable}
              {...text("enquiry_date")}
              className="w-40"
            />
            <Select
              label={t("adm.field.class")}
              value={form.class_id ?? ""}
              disabled={!editable}
              onChange={(e) => set("class_id", e.target.value || null)}
              options={[
                { value: "", label: "—" },
                ...(classes.data ?? []).map((c) => ({ value: c.id, label: c.name })),
              ]}
              className="w-40"
            />
            {manage && (
              <Select
                label={id ? t("adm.move") : t("adm.field.stage")}
                value={id ? (d?.stage_id ?? "") : form.stage_id}
                disabled={!manage || converted}
                onChange={(e) => (id ? void move(e.target.value) : set("stage_id", e.target.value))}
                options={[
                  ...(id ? [] : [{ value: "", label: board.stages[0]?.name ?? "—" }]),
                  ...board.stages.map((s) => ({ value: s.id, label: s.name })),
                ]}
                className="w-44"
              />
            )}
            <Input
              label={t("adm.field.source")}
              disabled={!editable}
              {...text("source")}
              className="w-36"
            />
          </div>
          <section>
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted">
              {t("adm.section.applicant")}
            </h3>
            <div className="grid grid-cols-2 gap-3 md:grid-cols-3">
              <Input label={t("adm.field.name")} disabled={!editable} {...text("full_name")} />
              <Input label={t("adm.field.father")} disabled={!editable} {...text("father_name")} />
              <Input
                label={t("adm.field.dob")}
                type="date"
                disabled={!editable}
                {...text("date_of_birth")}
              />
              <Select
                label={t("adm.field.gender")}
                value={form.gender}
                disabled={!editable}
                onChange={(e) => set("gender", e.target.value)}
                options={[
                  { value: "", label: "—" },
                  ...GENDERS.map((g) => ({ value: g, label: genderLabel(t, g) })),
                ]}
              />
              <Input label={t("adm.field.cnic")} disabled={!editable} {...text("cnic")} />
              <Input label={t("adm.field.phone")} disabled={!editable} {...text("phone")} />
              <Input label={t("adm.field.email")} disabled={!editable} {...text("email")} />
              <Input
                label={t("adm.field.previousSchool")}
                disabled={!editable}
                {...text("previous_school")}
              />
              <Input label={t("adm.field.address")} disabled={!editable} {...text("address")} />
            </div>
          </section>
          <section>
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted">
              {t("adm.field.guardian")}
            </h3>
            <div className="grid grid-cols-2 gap-3 md:grid-cols-3">
              <Input
                label={t("adm.field.guardianName")}
                disabled={!editable}
                {...text("guardian_name")}
              />
              <Select
                label={t("adm.field.relationship")}
                value={form.guardian_relationship}
                disabled={!editable}
                onChange={(e) => set("guardian_relationship", e.target.value)}
                options={RELATIONSHIPS.map((r) => ({ value: r, label: relationshipLabel(t, r) }))}
              />
              <Input
                label={t("adm.field.guardianPhone")}
                disabled={!editable}
                {...text("guardian_phone")}
              />
              <Input
                label={t("adm.field.guardianCnic")}
                disabled={!editable}
                {...text("guardian_cnic")}
              />
              <Input
                label={t("adm.field.guardianEmail")}
                disabled={!editable}
                {...text("guardian_email")}
              />
              <Input
                label={t("adm.field.occupation")}
                disabled={!editable}
                {...text("guardian_occupation")}
              />
            </div>
          </section>
          <section>
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted">
              {t("adm.section.test")}
            </h3>
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              <Input
                label={t("adm.field.testDate")}
                type="date"
                disabled={!editable}
                {...text("test_date")}
              />
              <Input
                label={t("adm.field.testMarks")}
                type="number"
                step="0.5"
                disabled={!editable}
                {...text("test_marks")}
              />
              <Input
                label={t("adm.field.testTotal")}
                type="number"
                disabled={!editable}
                {...text("test_total")}
              />
              <Input
                label={t("adm.field.interviewDate")}
                type="date"
                disabled={!editable}
                {...text("interview_date")}
              />
              <div className="col-span-2 md:col-span-4">
                <Input
                  label={t("adm.field.interviewNotes")}
                  disabled={!editable}
                  {...text("interview_notes")}
                />
              </div>
            </div>
          </section>
          <section>
            <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted">
              {t("adm.section.offer")}
            </h3>
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              <Input
                label={t("adm.field.offerDate")}
                type="date"
                disabled={!editable}
                {...text("offer_date")}
              />
              <Input
                label={t("adm.field.offerFee")}
                type="number"
                step="0.01"
                disabled={!editable}
                {...text("offer_fee")}
              />
              <Input
                label={t("adm.field.offerExpires")}
                type="date"
                disabled={!editable}
                {...text("offer_expires")}
              />
              <Input label={t("adm.field.notes")} disabled={!editable} {...text("notes")} />
            </div>
          </section>
          {d && d.events.length > 0 && (
            <section>
              <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted">
                {t("adm.history")}
              </h3>
              <ul className="flex flex-col gap-1 text-xs" data-testid="history">
                {d.events.map((e) => (
                  <li key={e.id} className="flex flex-wrap gap-2">
                    <span className="text-muted">{formatDate(e.changed_at)}</span>
                    <span className="font-medium">{e.to_stage_name}</span>
                    {e.changed_by_name && <span className="text-muted">{e.changed_by_name}</span>}
                    {e.note && <span>{e.note}</span>}
                  </li>
                ))}
              </ul>
            </section>
          )}
        </div>
      )}
      {converting && d && (
        <ConvertDialog
          application={d}
          onClose={() => setConverting(false)}
          onDone={(saved) => {
            detail.setData(saved);
            setConverting(false);
            onChanged();
          }}
        />
      )}
    </Dialog>
  );
}

function ConvertDialog({
  application,
  onClose,
  onDone,
}: {
  application: ApplicationDetail;
  onClose: () => void;
  onDone: (d: ApplicationDetail) => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const sections = useAsync(
    () =>
      api.sections({
        academic_year_id: application.academic_year_id,
        class_id: application.class_id ?? undefined,
      }),
    [application.academic_year_id, application.class_id],
  );
  const [sectionId, setSectionId] = useState("");
  const [rollNo, setRollNo] = useState("");
  const [date, setDate] = useState(application.admitted_on ?? today());
  const [number, setNumber] = useState("");
  const [force, setForce] = useState(false);
  const [busy, setBusy] = useState(false);
  const section = sectionId || sections.data?.[0]?.id || "";
  const convert = async () => {
    setBusy(true);
    try {
      const r = await api.convertApplication(application.id, {
        section_id: section,
        roll_no: rollNo === "" ? null : Number(rollNo),
        admission_date: date || null,
        student_number: number || null,
        force,
      });
      toast.push({ kind: "success", title: t("adm.convert.done", { number: r.student_number }) });
      onDone(r.application);
    } catch (err) {
      toast.push({ kind: "error", title: errorMessage(err, t("app.error.generic")) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={`${t("adm.convert")}: ${application.full_name}`}
      footer={
        <div className="flex gap-2">
          <Button variant="secondary" onClick={onClose}>
            {t("action.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={!section}
            onClick={() => void convert()}
            data-testid="convert-confirm"
          >
            {t("adm.convert")}
          </Button>
        </div>
      }
    >
      <p className="mb-3 text-xs text-muted">{t("adm.convert.hint")}</p>
      <div className="grid grid-cols-2 gap-3">
        <Select
          label={t("adm.convert.section")}
          value={section}
          onChange={(e) => setSectionId(e.target.value)}
          options={(sections.data ?? []).map((s) => ({ value: s.id, label: s.name }))}
        />
        <Input
          label={t("adm.convert.rollNo")}
          type="number"
          value={rollNo}
          onChange={(e) => setRollNo(e.target.value)}
        />
        <Input
          label={t("adm.convert.admissionDate")}
          type="date"
          value={date}
          onChange={(e) => setDate(e.target.value)}
        />
        <Input
          label={t("adm.convert.studentNumber")}
          value={number}
          onChange={(e) => setNumber(e.target.value)}
        />
        <label className="col-span-2 flex items-center gap-2 text-sm">
          <input type="checkbox" checked={force} onChange={(e) => setForce(e.target.checked)} />
          {t("adm.convert.force")}
        </label>
      </div>
    </Dialog>
  );
}
