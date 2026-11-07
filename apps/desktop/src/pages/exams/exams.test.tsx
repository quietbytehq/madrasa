import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import { en } from "@/i18n/en";
import type {
  AssessmentType,
  MarksSheetView,
  ReportCard,
  ResultSheet,
  StatusView,
  UserSummary,
  Term,
  ExamStructure,
} from "@/api/types";
import { MarksTab } from "./tabs/MarksTab";
import { SetupTab } from "./tabs/SetupTab";
import { ResultSheetView } from "./tabs/ResultsTab";
import { ReportCardView } from "./ReportCardView";

vi.mock("@/api/endpoints", () => ({
  api: {
    academicYears: vi.fn(),
    assessments: vi.fn(),
    attendanceStatus: vi.fn(),
    examSheets: vi.fn(),
    examSheet: vi.fn(),
    saveMarks: vi.fn(),
    downloadMarksTemplate: vi.fn(async () => undefined),
    uploadMarks: vi.fn(),
    submitSheet: vi.fn(),
    approveSheet: vi.fn(),
    reopenSheet: vi.fn(),
    grading: vi.fn(),
    setGrading: vi.fn(),
    maxMarks: vi.fn(),
    setMaxMarks: vi.fn(),
    setWeights: vi.fn(),
    defaultAssessments: vi.fn(),
    createAssessment: vi.fn(),
    renameAssessment: vi.fn(),
    deleteAssessment: vi.fn(),
    examStructure: vi.fn(),
    terms: vi.fn(),
    createTerm: vi.fn(),
    updateTerm: vi.fn(),
    deleteTerm: vi.fn(),
    setTermWeights: vi.fn(),
    classes: vi.fn(async () => [{ id: "c1", school_id: "s", name: "1", level: 1 }]),
  },
}));

const t = (key: keyof typeof en, params?: Record<string, string | number>) => {
  let s: string = en[key];
  for (const [k, v] of Object.entries(params ?? {})) s = s.replace(`{${k}}`, String(v));
  return s;
};

const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "sana",
  full_name: "Sana Malik",
  email: null,
  is_active: true,
  roles: ["teacher"],
};
const TEACHER = ["exams.read", "exams.manage", "academics.read", "attendance.read"];
const ADMIN = [...TEACHER, "academics.manage", "exams.approve"];

const assessments: AssessmentType[] = [
  {
    id: "a1",
    school_id: "s",
    academic_year_id: "y",
    name: "Bi-monthly Test 1",
    position: 0,
    weight: "15.00",
    term_id: "t1",
    kind: "entered",
    formula: null,
  },
  {
    id: "a2",
    school_id: "s",
    academic_year_id: "y",
    name: "Final",
    position: 1,
    weight: "85.00",
    term_id: "t1",
    kind: "entered",
    formula: null,
  },
];
const term1: Term = {
  id: "t1",
  school_id: "s",
  academic_year_id: "y",
  name: "Year",
  position: 0,
  starts_on: null,
  ends_on: null,
  weight: "100.00",
};
const structure: ExamStructure = {
  terms: [{ ...term1, components: assessments }],
  unassigned: [],
};
const status: StatusView = {
  date: "2026-04-06",
  day_kind: { kind: "school" },
  sections: [
    {
      section_id: "sec1",
      class_id: "c1",
      class_name: "1",
      section_name: "A",
      class_teacher_name: null,
      enrolled: 2,
      sheet_id: null,
      submitted_at: null,
      present: null,
      absent: null,
      late: null,
      leave: null,
    },
  ],
  totals: {
    sections: 1,
    marked: 0,
    enrolled: 2,
    present: 0,
    absent: 0,
    late: 0,
    leave: 0,
    percent: null,
  },
};
const sheet: MarksSheetView = {
  assessment: assessments[0]!,
  section: {
    id: "sec1",
    class_id: "c1",
    class_name: "1",
    section_name: "A",
    class_teacher_name: null,
  },
  subject_id: "eng",
  subject_name: "English",
  max_marks: "50.00",
  sheet: null,
  status: "draft",
  submitted_by_name: null,
  approved_by_name: null,
  students: [
    {
      student_id: "ali",
      student_number: "KHI-MC-2026-00001",
      full_name: "Ali Khan",
      roll_no: 1,
      obtained: null,
      is_absent: false,
    },
    {
      student_id: "sara",
      student_number: "KHI-MC-2026-00002",
      full_name: "Sara Khan",
      roll_no: 2,
      obtained: null,
      is_absent: false,
    },
  ],
  can_edit: true,
  can_submit: false,
  can_approve: false,
  can_reopen: false,
};

beforeEach(() => {
  vi.mocked(api.academicYears).mockResolvedValue([
    {
      id: "y",
      school_id: "s",
      name: "2026-27",
      start_date: "2026-04-01",
      end_date: "2027-03-31",
      start_month: 4,
      is_active: true,
    },
  ]);
  vi.mocked(api.assessments).mockResolvedValue(assessments);
  vi.mocked(api.examStructure).mockResolvedValue(structure);
  vi.mocked(api.attendanceStatus).mockResolvedValue(status);
  vi.mocked(api.examSheets).mockResolvedValue({
    assessment: assessments[0]!,
    section: sheet.section,
    sheets: [
      {
        subject_id: "eng",
        subject_name: "English",
        subject_code: "ENG",
        max_marks: "50.00",
        sheet_id: null,
        status: null,
        submitted_at: null,
        approved_at: null,
        entered: 0,
        students: 2,
        can_enter: true,
      },
      {
        subject_id: "mth",
        subject_name: "Mathematics",
        subject_code: "MTH",
        max_marks: null,
        sheet_id: null,
        status: null,
        submitted_at: null,
        approved_at: null,
        entered: 0,
        students: 2,
        can_enter: false,
      },
    ],
  });
  vi.mocked(api.examSheet).mockResolvedValue(sheet);
  vi.mocked(api.saveMarks).mockClear();
  vi.mocked(api.saveMarks).mockImplementation(async (body) => ({
    ...sheet,
    sheet: {
      id: "sh1",
      assessment_type_id: "a1",
      section_id: "sec1",
      subject_id: "eng",
      status: "draft",
      submitted_by: null,
      submitted_at: null,
      approved_by: null,
      approved_at: null,
    },
    can_submit: true,
    students: sheet.students.map((s) => {
      const m = body.marks.find((x) => x.student_id === s.student_id);
      return { ...s, obtained: m?.obtained ?? null, is_absent: m?.is_absent ?? false };
    }),
  }));
  vi.mocked(api.submitSheet).mockClear();
  vi.mocked(api.submitSheet).mockResolvedValue({
    ...sheet,
    status: "submitted",
    can_edit: false,
    can_submit: false,
    submitted_by_name: "Sana Malik",
    sheet: {
      id: "sh1",
      assessment_type_id: "a1",
      section_id: "sec1",
      subject_id: "eng",
      status: "submitted",
      submitted_by: "u1",
      submitted_at: "2026-05-01T05:00:00Z",
      approved_by: null,
      approved_at: null,
    },
  });
});

describe("MarksTab", () => {
  it("lists subjects, validates marks against the maximum and saves, then submits", async () => {
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: TEACHER }}>
        <MarksTab />
      </SessionProvider>,
    );
    expect(await screen.findByText("English")).toBeInTheDocument();
    expect(screen.getByText("Maximum marks not set")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Open" })).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const ali = await screen.findByLabelText("Ali Khan: Marks");
    await userEvent.type(ali, "60");
    expect(await screen.findByRole("alert")).toHaveTextContent("between 0 and 50");
    expect(screen.getByRole("button", { name: "Save marks" })).toBeDisabled();
    await userEvent.clear(ali);
    await userEvent.type(ali, "45{Enter}");
    expect(screen.getByLabelText("Sara Khan: Marks")).toHaveFocus();
    await userEvent.click(screen.getByLabelText("Sara Khan: Absent"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Save marks" }));
    await waitFor(() =>
      expect(api.saveMarks).toHaveBeenCalledWith({
        assessment_id: "a1",
        section_id: "sec1",
        subject_id: "eng",
        marks: [
          { student_id: "ali", obtained: "45", is_absent: false },
          { student_id: "sara", obtained: null, is_absent: true },
        ],
      }),
    );
    expect(await screen.findByText("Marks saved.")).toBeInTheDocument();
    await userEvent.click(await screen.findByRole("button", { name: "Submit for approval" }));
    await waitFor(() => expect(api.submitSheet).toHaveBeenCalledWith("sh1"));
    expect(await screen.findByText("Submitted")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Save marks" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Ali Khan: Marks")).toBeDisabled();
  });

  it("downloads the Excel template and uploads a filled sheet through validation", async () => {
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: TEACHER }}>
        <MarksTab />
      </SessionProvider>,
    );
    await userEvent.click(await screen.findByRole("button", { name: "Open" }));
    await screen.findByLabelText("Ali Khan: Marks");
    await userEvent.click(screen.getByRole("button", { name: "Download Excel sheet" }));
    await waitFor(() =>
      expect(api.downloadMarksTemplate).toHaveBeenCalledWith("a1", "sec1", "eng"),
    );

    // A sheet with a bad value: nothing is saved and the row is pointed out.
    vi.mocked(api.uploadMarks).mockResolvedValueOnce({
      sheet: null,
      saved: 0,
      issues: [{ row_no: 3, message: "Marks must be between 0 and 50" }],
    });
    const file = new File(["marks"], "marks.xlsx", {
      type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    });
    await userEvent.upload(screen.getByLabelText("Upload filled sheet"), file);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Row 3: Marks must be between 0 and 50",
    );
    expect(api.uploadMarks).toHaveBeenCalledWith(
      expect.objectContaining({
        assessment_id: "a1",
        section_id: "sec1",
        subject_id: "eng",
        file_name: "marks.xlsx",
        content_base64: btoa("marks"),
      }),
    );

    // A clean sheet lands in the same draft as on-screen entry.
    vi.mocked(api.uploadMarks).mockResolvedValueOnce({
      sheet: {
        ...sheet,
        sheet: {
          id: "sh1",
          assessment_type_id: "a1",
          section_id: "sec1",
          subject_id: "eng",
          status: "draft",
          submitted_by: null,
          submitted_at: null,
          approved_by: null,
          approved_at: null,
        },
        can_submit: true,
        students: [
          { ...sheet.students[0]!, obtained: "45.00" },
          { ...sheet.students[1]!, obtained: null, is_absent: true },
        ],
      },
      saved: 2,
      issues: [],
    });
    const fixed = new File(["marks2"], "marks.xlsx", { type: file.type });
    await userEvent.upload(screen.getByLabelText("Upload filled sheet"), fixed);
    expect(
      await screen.findByText("2 marks read from the file and saved as a draft."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Ali Khan: Marks")).toHaveValue("45");
    expect(screen.getByLabelText("Sara Khan: Absent")).toBeChecked();
    expect(screen.getByRole("button", { name: "Submit for approval" })).toBeInTheDocument();
  });

  it("offers approve and reopen to the principal only", async () => {
    vi.mocked(api.examSheet).mockResolvedValue({
      ...sheet,
      status: "submitted",
      can_edit: false,
      can_approve: true,
      can_reopen: true,
      sheet: {
        id: "sh1",
        assessment_type_id: "a1",
        section_id: "sec1",
        subject_id: "eng",
        status: "submitted",
        submitted_by: "u1",
        submitted_at: "2026-05-01T05:00:00Z",
        approved_by: null,
        approved_at: null,
      },
      submitted_by_name: "Sana Malik",
    });
    vi.mocked(api.approveSheet).mockResolvedValue({
      ...sheet,
      status: "approved",
      can_edit: false,
      can_reopen: true,
      sheet: {
        id: "sh1",
        assessment_type_id: "a1",
        section_id: "sec1",
        subject_id: "eng",
        status: "approved",
        submitted_by: "u1",
        submitted_at: "2026-05-01T05:00:00Z",
        approved_by: "u2",
        approved_at: "2026-05-02T05:00:00Z",
      },
    });
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <MarksTab />
      </SessionProvider>,
    );
    await userEvent.click(await screen.findByRole("button", { name: "Open" }));
    expect(await screen.findByText(/Submitted by Sana Malik/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(api.approveSheet).toHaveBeenCalledWith("sh1"));
    expect(await screen.findByText("Approved")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reopen" })).toBeInTheDocument();
  });
});

function singleTermCard(): ReportCard {
  return {
    school: { name: "Gulshan High", code: "GHS", city: "Karachi" },
    academic_year: "2026-27",
    student_id: "ali",
    student_number: "KHI-MC-2026-00001",
    full_name: "Ali Khan",
    father_name: "Ahmed Khan",
    class_name: "1",
    section_name: "A",
    roll_no: 1,
    assessments: [
      {
        id: "a1",
        name: "Bi-monthly Test 1",
        weight: "15.00",
        held: true,
        term_id: "t1",
        kind: "entered",
      },
      { id: "a2", name: "Final", weight: "85.00", held: false, term_id: "t1", kind: "entered" },
    ],
    subjects: [
      {
        subject_id: "eng",
        subject_name: "English",
        cells: [
          {
            max_marks: "50.00",
            obtained: "45.00",
            is_absent: false,
            percent: "90.00",
            approved: true,
          },
          { max_marks: null, obtained: null, is_absent: false, percent: null, approved: false },
        ],
        weighted_percent: "90.00",
        term_percents: [],
        grade: "A+",
      },
    ],
    terms: [],
    totals: [
      {
        assessment_id: "a1",
        total: "45.00",
        max: "50.00",
        percent: "90.00",
        grade: "A+",
        position: 1,
        students: 30,
      },
      {
        assessment_id: "a2",
        total: "0.00",
        max: "0.00",
        percent: null,
        grade: null,
        position: null,
        students: 0,
      },
    ],
    overall_percent: "90.00",
    overall_grade: "A+",
    overall_remarks: "Outstanding",
    pass: true,
    attendance: { present: 45, total: 47, percent: 95.7 },
    grading: { bands: [], pass_percent: "40" },
  };
}

describe("ResultSheetView and ReportCardView", () => {
  it("renders totals, grades, positions and analysis", () => {
    const r: ResultSheet = {
      assessment: assessments[0]!,
      section: sheet.section,
      subjects: [
        {
          subject_id: "eng",
          subject_name: "English",
          max_marks: "50.00",
          sheet_status: "approved",
        },
        {
          subject_id: "mth",
          subject_name: "Mathematics",
          max_marks: "50.00",
          sheet_status: "submitted",
        },
      ],
      rows: [
        {
          student_id: "ali",
          student_number: "1",
          full_name: "Ali Khan",
          roll_no: 1,
          marks: [
            {
              subject_id: "eng",
              takes: true,
              obtained: "45.00",
              is_absent: false,
              percent: "90.00",
              grade: "A+",
            },
            {
              subject_id: "mth",
              takes: true,
              obtained: "40.00",
              is_absent: false,
              percent: "80.00",
              grade: "A",
            },
          ],
          total: "85.00",
          max: "100.00",
          percent: "85.00",
          grade: "A",
          pass: true,
          position: 1,
        },
        {
          student_id: "sara",
          student_number: "2",
          full_name: "Sara Khan",
          roll_no: 2,
          marks: [
            {
              subject_id: "eng",
              takes: true,
              obtained: null,
              is_absent: true,
              percent: "0.00",
              grade: "F",
            },
            {
              subject_id: "mth",
              takes: true,
              obtained: "25.00",
              is_absent: false,
              percent: "50.00",
              grade: "D",
            },
          ],
          total: "25.00",
          max: "100.00",
          percent: "25.00",
          grade: "F",
          pass: false,
          position: 2,
        },
      ],
      analysis: [
        {
          subject_id: "eng",
          subject_name: "English",
          max_marks: "50.00",
          appeared: 1,
          absent: 1,
          average_percent: "90.00",
          highest: "45.00",
          lowest: "45.00",
          passed: 1,
          failed: 0,
        },
      ],
      grading: { bands: [], pass_percent: "40" },
      complete: false,
    };
    renderWithProviders(<ResultSheetView r={r} t={t as never} />);
    expect(screen.getByRole("status")).toHaveTextContent("not approved yet");
    const aliRow = screen.getByText("Ali Khan").closest("tr")!;
    expect(within(aliRow).getByText("85/100")).toBeInTheDocument();
    expect(within(aliRow).getByText("85.00%")).toBeInTheDocument();
    expect(within(aliRow).getByText("A")).toBeInTheDocument();
    const saraRow = screen.getByText("Sara Khan").closest("tr")!;
    expect(within(saraRow).getByText("A", { selector: "td" })).toHaveClass("text-danger");
    expect(saraRow.lastElementChild).toHaveTextContent("2");
    expect(screen.getByText("90.00%", { selector: "td" })).toBeInTheDocument();
  });

  it("renders a report card with weighted results, position, attendance and signatures", () => {
    const card = singleTermCard();
    renderWithProviders(<ReportCardView card={card} t={t as never} />);
    expect(screen.getByText("Gulshan High")).toBeInTheDocument();
    expect(screen.getAllByText("45/50")).toHaveLength(2);
    expect(screen.getAllByText("90.00%").length).toBeGreaterThan(0);
    expect(screen.getByText("Position 1 of 30")).toBeInTheDocument();
    expect(screen.getByText("Pass")).toBeInTheDocument();
    expect(screen.getByText("Outstanding")).toBeInTheDocument();
    expect(screen.getByText("45/47 (95.7%)")).toBeInTheDocument();
    expect(screen.getByText("Principal")).toBeInTheDocument();
    expect(screen.queryByTestId("term-header")).not.toBeInTheDocument();
  });

  it("groups columns by term and shows term results on a two-term card", () => {
    const base = singleTermCard();
    const card: ReportCard = {
      ...base,
      assessments: [
        { ...base.assessments[0]!, term_id: "t1" },
        { ...base.assessments[1]!, term_id: "t2" },
      ],
      subjects: base.subjects.map((s) => ({ ...s, term_percents: ["90.00", null] })),
      terms: [
        {
          id: "t1",
          name: "Term 1",
          weight: "50.00",
          component_ids: ["a1"],
          percent: "90.00",
          grade: "A+",
        },
        {
          id: "t2",
          name: "Term 2",
          weight: "50.00",
          component_ids: ["a2"],
          percent: null,
          grade: null,
        },
      ],
    };
    renderWithProviders(<ReportCardView card={card} t={t as never} />);
    const header = screen.getByTestId("term-header");
    expect(within(header).getByText("Term 1")).toBeInTheDocument();
    expect(within(header).getByText("Term 2")).toBeInTheDocument();
    const result = screen.getByTestId("term-result");
    expect(result).toHaveTextContent("Term result");
    expect(result).toHaveTextContent("90.00% · A+");
    expect(result).toHaveTextContent("—");
  });
});

describe("SetupTab", () => {
  it("only saves weights that total 100", async () => {
    vi.mocked(api.grading).mockResolvedValue({
      bands: [{ grade: "A", min_percent: "0", remarks: "" }],
      pass_percent: "40",
    });
    vi.mocked(api.maxMarks).mockResolvedValue([]);
    vi.mocked(api.setWeights).mockResolvedValue(assessments);
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <SetupTab />
      </SessionProvider>,
    );
    const w1 = await screen.findByLabelText("Bi-monthly Test 1: Weight %");
    const save = screen.getByRole("button", { name: "Save weights" });
    expect(save).toBeEnabled();
    await userEvent.clear(w1);
    await userEvent.type(w1, "20");
    expect(screen.getByText("Total 105%")).toBeInTheDocument();
    expect(save).toBeDisabled();
    const w2 = screen.getByLabelText("Final: Weight %");
    await userEvent.clear(w2);
    await userEvent.type(w2, "80");
    expect(screen.getByText("Total 100%")).toBeInTheDocument();
    await userEvent.click(save);
    await waitFor(() =>
      expect(api.setWeights).toHaveBeenCalledWith("y", [
        { id: "a1", weight: "20" },
        { id: "a2", weight: "80" },
      ]),
    );
  });

  it("offers one- or two-term defaults when the year is empty", async () => {
    vi.mocked(api.assessments).mockResolvedValue([]);
    vi.mocked(api.examStructure).mockResolvedValue({ terms: [], unassigned: [] });
    vi.mocked(api.grading).mockResolvedValue({ bands: [], pass_percent: "40" });
    vi.mocked(api.maxMarks).mockResolvedValue([]);
    vi.mocked(api.defaultAssessments).mockResolvedValue([]);
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <SetupTab />
      </SessionProvider>,
    );
    const terms = await screen.findByLabelText("Terms");
    expect(terms).toHaveValue("2");
    await userEvent.selectOptions(terms, "1");
    await userEvent.click(screen.getByRole("button", { name: "Add default assessments" }));
    await waitFor(() => expect(api.defaultAssessments).toHaveBeenCalledWith("y", 1));
  });

  it("creates a calculated component from the formula editor", async () => {
    vi.mocked(api.grading).mockResolvedValue({ bands: [], pass_percent: "40" });
    vi.mocked(api.maxMarks).mockResolvedValue([]);
    vi.mocked(api.createAssessment).mockResolvedValue({
      ...assessments[0]!,
      id: "a3",
      name: "Best test",
      kind: "calculated",
    });
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <SetupTab />
      </SessionProvider>,
    );
    expect(await screen.findByText("Year")).toBeInTheDocument();
    expect(screen.getByLabelText("Year: Weight in year (%)")).toHaveValue("100");
    await userEvent.click(screen.getByRole("button", { name: "Add calculated component" }));
    const editor = screen.getByTestId("formula-editor");
    await userEvent.type(within(editor).getByLabelText("Name"), "Best test");
    await userEvent.selectOptions(within(editor).getByLabelText("Formula"), "best_of");
    await userEvent.clear(within(editor).getByLabelText("N"));
    await userEvent.type(within(editor).getByLabelText("N"), "1");
    await userEvent.click(within(editor).getByRole("checkbox", { name: "Bi-monthly Test 1" }));
    await userEvent.click(within(editor).getByRole("checkbox", { name: "Final" }));
    await userEvent.click(within(editor).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(api.createAssessment).toHaveBeenCalledWith({
        academic_year_id: "y",
        name: "Best test",
        position: 2,
        term_id: "t1",
        kind: "calculated",
        weight: "0",
        formula: {
          op: "best_of",
          inputs: [
            { assessment_type_id: "a1", weight: null },
            { assessment_type_id: "a2", weight: null },
          ],
          n: 1,
        },
      }),
    );
  });
});
