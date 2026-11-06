import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type {
  BoardCandidate,
  BoardExamView,
  BoardImportPreview,
  BoardParsedFile,
  BoardResultSheet,
  ExamBoard,
  UserSummary,
} from "@/api/types";
import { BoardsTab } from "./tabs/BoardsTab";

vi.mock("@/api/endpoints", () => ({
  api: {
    academicYears: vi.fn(async () => [
      {
        id: "y1",
        school_id: "s",
        name: "2026-27",
        start_date: "2026-04-01",
        end_date: "2027-03-31",
        start_month: 4,
        is_active: true,
      },
    ]),
    assessments: vi.fn(async () => []),
    attendanceStatus: vi.fn(async () => ({
      date: "2026-11-06",
      day_kind: { kind: "school" },
      sections: [],
      totals: {
        sections: 0,
        marked: 0,
        enrolled: 0,
        present: 0,
        absent: 0,
        late: 0,
        leave: 0,
        percent: null,
      },
    })),
    classes: vi.fn(async () => [
      { id: "c9", school_id: "s", name: "9", level: 9 },
      { id: "c10", school_id: "s", name: "10", level: 10 },
    ]),
    subjects: vi.fn(async () => [{ id: "eng", school_id: "s", name: "English", code: "ENG" }]),
    boards: vi.fn(),
    createBoard: vi.fn(),
    updateBoard: vi.fn(),
    deleteBoard: vi.fn(),
    boardExams: vi.fn(),
    createBoardExam: vi.fn(),
    updateBoardExam: vi.fn(),
    deleteBoardExam: vi.fn(),
    boardCandidates: vi.fn(),
    registerCandidates: vi.fn(),
    updateCandidate: vi.fn(),
    removeCandidate: vi.fn(),
    boardResults: vi.fn(),
    parseBoardResults: vi.fn(),
    previewBoardResults: vi.fn(),
    applyBoardResults: vi.fn(),
    rollbackImport: vi.fn(),
  },
}));

const board: ExamBoard = {
  id: "b1",
  school_id: "s",
  name: "BISE Karachi",
  short_code: "BISEK",
  country: "PK",
  grading_mode: "marks",
  pass_percent: "33.00",
  grade_bands: [{ grade: "A", min_percent: "70.00" }],
};
const exam: BoardExamView = {
  id: "x1",
  school_id: "s",
  board_id: "b1",
  academic_year_id: "y1",
  name: "SSC Part I Annual 2026",
  status: "announced",
  board_name: "BISE Karachi",
  academic_year_name: "2026-27",
  class_ids: ["c9"],
  class_names: ["9"],
  subjects: [
    {
      id: "p1",
      board_exam_id: "x1",
      name: "English",
      subject_id: "eng",
      max_marks: "75.00",
      position: 0,
    },
    {
      id: "p2",
      board_exam_id: "x1",
      name: "Mathematics",
      subject_id: null,
      max_marks: "75.00",
      position: 1,
    },
  ],
  candidates: 1,
  results: 0,
};
const ali: BoardCandidate = {
  id: "cand1",
  board_exam_id: "x1",
  student_id: "st1",
  roll_no: "123001",
  registration_no: null,
  group_name: "Science",
  student_number: "KHI-MC-2026-00001",
  full_name: "Ali Khan",
  father_name: "Ahmed Khan",
  class_name: "9",
  section_name: "A",
  section_id: "sec9",
};
const emptySheet: BoardResultSheet = {
  exam,
  board,
  rows: [],
  subjects: [],
  candidates: 1,
  with_results: 0,
  passed: 0,
  pass_percent: null,
};
const parsed: BoardParsedFile = {
  file_name: "ssc.xlsx",
  sheets: ["Sheet1"],
  sheet: "Sheet1",
  headers: ["Roll No", "English", "Mathematics"],
  rows: [["123001", "60", "70"]],
  mapping: { roll_no: 0, "marks:p1": 1, "marks:p2": 2 },
  layout: "wide",
  mapping_saved: false,
};
const preview: BoardImportPreview = {
  rows: [
    {
      row_no: 2,
      roll_no: "123001",
      candidate_id: "cand1",
      candidate_name: "Ali Khan",
      results: [
        {
          board_subject_id: "p1",
          subject: "English",
          obtained: "60",
          max_marks: "75.00",
          grade: "A",
          status: "pass",
          replaces: false,
        },
        {
          board_subject_id: "p2",
          subject: "Mathematics",
          obtained: "70",
          max_marks: "75.00",
          grade: "A",
          status: "pass",
          replaces: false,
        },
      ],
      errors: [],
      warnings: [],
    },
  ],
  summary: { rows: 1, candidates: 1, results: 2, replaced: 0, errors: 0 },
  unmapped: [],
};
const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "ayesha",
  full_name: "Ayesha",
  email: null,
  is_active: true,
  roles: ["admin"],
};

function render(permissions: string[]) {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions }}>
      <BoardsTab />
    </SessionProvider>,
  );
}

beforeEach(() => {
  vi.mocked(api.boards).mockResolvedValue([board]);
  vi.mocked(api.boardExams).mockResolvedValue([exam]);
  vi.mocked(api.boardCandidates).mockResolvedValue([ali]);
  vi.mocked(api.boardResults).mockResolvedValue(emptySheet);
  vi.mocked(api.createBoard).mockReset();
  vi.mocked(api.createBoardExam).mockReset();
  vi.mocked(api.registerCandidates).mockReset();
  vi.mocked(api.parseBoardResults).mockReset();
  vi.mocked(api.previewBoardResults).mockReset();
  vi.mocked(api.applyBoardResults).mockReset();
});

describe("BoardsTab", () => {
  it("lists boards and their exams, and adds a board with grade bands", async () => {
    vi.mocked(api.createBoard).mockResolvedValue({
      ...board,
      id: "b2",
      name: "Federal Board",
      short_code: "FBISE",
    });
    render(["exams.read", "academics.manage"]);
    expect(await screen.findByText("BISE Karachi")).toBeInTheDocument();
    expect(await screen.findByText("SSC Part I Annual 2026")).toBeInTheDocument();
    expect(screen.getByText("Announced")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Add board" }));
    await userEvent.type(screen.getByLabelText("Board name"), "Federal Board");
    await userEvent.type(screen.getByLabelText("Short code"), "FBISE");
    expect(screen.getByLabelText("Grade 1")).toHaveValue("A+");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(api.createBoard).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Federal Board",
          short_code: "FBISE",
          pass_percent: "33",
          grade_bands: expect.arrayContaining([{ grade: "A+", min_percent: "80" }]),
        }),
      ),
    );
    expect(await screen.findByText("Board saved.")).toBeInTheDocument();
  });

  it("creates an exam with classes and papers", async () => {
    vi.mocked(api.createBoardExam).mockResolvedValue({ ...exam, id: "x2", name: "SSC Part II" });
    render(["exams.read", "academics.manage"]);
    await screen.findByText("SSC Part I Annual 2026");
    await userEvent.click(screen.getByRole("button", { name: "Add exam" }));
    await userEvent.type(screen.getByLabelText("Exam name"), "SSC Part II");
    await userEvent.click(screen.getByLabelText("10"));
    await userEvent.selectOptions(screen.getByLabelText("School subject 1"), "eng");
    expect(screen.getByLabelText("Paper 1")).toHaveValue("English");
    await userEvent.clear(screen.getByLabelText("Max marks 1"));
    await userEvent.type(screen.getByLabelText("Max marks 1"), "75");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(api.createBoardExam).toHaveBeenCalledWith({
        board_id: "b1",
        academic_year_id: "y1",
        name: "SSC Part II",
        status: "announced",
        class_ids: ["c10"],
        subjects: [{ name: "English", subject_id: "eng", max_marks: "75" }],
      }),
    );
  });

  it("registers a class, edits a roll number, and imports results through check and import", async () => {
    vi.mocked(api.registerCandidates).mockResolvedValue({ added: 25, candidates: [ali] });
    vi.mocked(api.parseBoardResults).mockResolvedValue(parsed);
    vi.mocked(api.previewBoardResults).mockResolvedValue(preview);
    vi.mocked(api.applyBoardResults).mockResolvedValue({
      batch_id: "batch1",
      summary: preview.summary,
    });
    vi.mocked(api.updateCandidate).mockResolvedValue({ ...ali, roll_no: "123009" });
    render(["exams.read", "academics.manage"]);
    await userEvent.click(await screen.findByRole("button", { name: "SSC Part I Annual 2026" }));
    await userEvent.click(screen.getByRole("tab", { name: "Candidates" }));
    await userEvent.selectOptions(screen.getByLabelText("Whole class"), "c9");
    await userEvent.click(screen.getByRole("button", { name: "Register" }));
    await waitFor(() =>
      expect(api.registerCandidates).toHaveBeenCalledWith("x1", {
        class_id: "c9",
        group_name: undefined,
      }),
    );
    expect(await screen.findByText("25 candidates registered.")).toBeInTheDocument();
    const roll = await screen.findByLabelText("Ali Khan: Board roll no.");
    await userEvent.clear(roll);
    await userEvent.type(roll, "123009");
    await userEvent.tab();
    await waitFor(() =>
      expect(api.updateCandidate).toHaveBeenCalledWith("cand1", { roll_no: "123009" }),
    );

    await userEvent.click(screen.getByRole("tab", { name: "Results" }));
    const file = new File(["x"], "ssc.xlsx", {
      type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    });
    await userEvent.upload(screen.getByLabelText("Results file (Excel or CSV)"), file);
    await waitFor(() =>
      expect(api.parseBoardResults).toHaveBeenCalledWith(
        "x1",
        expect.objectContaining({ file_name: "ssc.xlsx" }),
      ),
    );
    expect(await screen.findByLabelText("Roll No")).toHaveValue("roll_no");
    expect(screen.getByLabelText("English")).toHaveValue("marks:p1");
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Check" }));
    await waitFor(() =>
      expect(api.previewBoardResults).toHaveBeenCalledWith(
        "x1",
        expect.objectContaining({ mapping: parsed.mapping, layout: "wide", save_mapping: true }),
      ),
    );
    expect(await screen.findByText(/1 rows · 1 candidates · 2 results/)).toBeInTheDocument();
    expect(screen.getByText(/English: 60 \(A\)/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Import" }));
    await waitFor(() => expect(api.applyBoardResults).toHaveBeenCalled());
    expect(
      await screen.findByText("Results imported: 2 results for 1 candidates."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo this import" })).toBeInTheDocument();
  });

  it("blocks the import while rows have errors and hides management from a teacher", async () => {
    vi.mocked(api.parseBoardResults).mockResolvedValue(parsed);
    vi.mocked(api.previewBoardResults).mockResolvedValue({
      ...preview,
      rows: [
        {
          ...preview.rows[0]!,
          candidate_id: null,
          candidate_name: null,
          results: [],
          errors: ["No candidate with roll number '123001' in this exam."],
        },
      ],
      summary: { ...preview.summary, results: 0, errors: 1 },
    });
    const first = render(["exams.read", "academics.manage"]);
    await userEvent.click(await screen.findByRole("button", { name: "SSC Part I Annual 2026" }));
    await userEvent.upload(
      screen.getByLabelText("Results file (Excel or CSV)"),
      new File(["x"], "ssc.xlsx"),
    );
    await userEvent.click(await screen.findByRole("button", { name: "Check" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Fix the rows with errors");
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();

    first.unmount();

    render(["exams.read"]);
    const lists = await screen.findAllByRole("list", { name: "Examination boards" });
    expect(
      within(lists[lists.length - 1]!).queryByRole("button", { name: /Edit board/ }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add board" })).not.toBeInTheDocument();
  });
});
