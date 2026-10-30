import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { ApiError } from "@/api/client";
import { SessionProvider } from "@/app/session";
import type { StudentDetail, StudentListRow, UserSummary } from "@/api/types";
import { StudentsPage } from "./StudentsPage";
import {
  StudentFormPage,
  toCreateRequest,
  toPatch,
  validateStudent,
  EMPTY_FORM,
} from "./StudentForm";
import { StudentProfile } from "./StudentProfile";

vi.mock("@/api/endpoints", () => ({
  api: {
    classes: vi.fn(async () => [{ id: "c1", school_id: "s", name: "1", level: 1 }]),
    sections: vi.fn(async () => [
      {
        id: "sec1",
        school_id: "s",
        class_id: "c1",
        academic_year_id: "y",
        name: "A",
        class_teacher_id: null,
        capacity: null,
        class_name: "1",
        class_level: 1,
        class_teacher_name: null,
        student_count: 2,
      },
    ]),
    students: vi.fn(),
    student: vi.fn(),
    createStudent: vi.fn(),
    updateStudent: vi.fn(),
    enrolStudent: vi.fn(),
    checkDuplicates: vi.fn(async () => []),
    searchGuardians: vi.fn(async () => []),
    withdrawStudent: vi.fn(),
    unlinkGuardian: vi.fn(),
    linkGuardian: vi.fn(),
  },
}));

const admin: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "admin",
  full_name: "Ayesha Khan",
  email: null,
  is_active: true,
  roles: ["admin"],
};
const perms = ["students.read", "students.manage", "academics.read"];

const rows: StudentListRow[] = [
  {
    id: "st1",
    student_number: "KHI-MC-2026-00001",
    full_name: "Ali Khan",
    father_name: "Ahmed Khan",
    gender: "male",
    date_of_birth: "2015-03-04",
    status: "active",
    admission_date: "2026-04-10",
    class_id: "c1",
    class_name: "1",
    section_id: "sec1",
    section_name: "A",
    roll_no: 7,
    has_photo: false,
  },
  {
    id: "st2",
    student_number: "KHI-MC-2026-00002",
    full_name: "Sara Khan",
    father_name: "Ahmed Khan",
    gender: "female",
    date_of_birth: "2017-05-06",
    status: "active",
    admission_date: "2026-04-11",
    class_id: "c1",
    class_name: "1",
    section_id: "sec1",
    section_name: "A",
    roll_no: 8,
    has_photo: false,
  },
];

const detail: StudentDetail = {
  id: "st1",
  school_id: "s",
  student_number: "KHI-MC-2026-00001",
  legacy_id: null,
  full_name: "Ali Khan",
  father_name: "Ahmed Khan",
  date_of_birth: "2015-03-04",
  gender: "male",
  cnic: "42101-1234567-1",
  address: "Gulshan",
  admission_date: "2026-04-10",
  status: "active",
  left_on: null,
  left_reason: null,
  board_roll_no: null,
  board_registration_no: null,
  has_photo: false,
  custom: { house: "Red" },
  guardians: [
    {
      id: "g1",
      school_id: "s",
      full_name: "Ahmed Khan",
      phone: "03001111111",
      email: null,
      cnic: null,
      occupation: "Trader",
      address: "",
      relationship: "father",
      is_primary: true,
    },
  ],
  enrollments: [
    {
      id: "e1",
      student_id: "st1",
      academic_year_id: "y",
      section_id: "sec1",
      roll_no: 7,
      academic_year_name: "2026-27",
      class_id: "c1",
      class_name: "1",
      section_name: "A",
    },
  ],
  current: {
    id: "e1",
    student_id: "st1",
    academic_year_id: "y",
    section_id: "sec1",
    roll_no: 7,
    academic_year_name: "2026-27",
    class_id: "c1",
    class_name: "1",
    section_name: "A",
  },
  siblings: [rows[1]!],
};

function renderAt(route: string, permissions = perms) {
  return renderWithProviders(
    <SessionProvider initial={{ user: admin, permissions }}>
      <Routes>
        <Route path="/students" element={<StudentsPage />} />
        <Route path="/students/new" element={<StudentFormPage />} />
        <Route path="/students/:id" element={<StudentProfile />} />
        <Route path="/students/:id/edit" element={<StudentFormPage />} />
      </Routes>
    </SessionProvider>,
    { route },
  );
}

describe("student form helpers", () => {
  it("validates required fields", () => {
    expect(validateStudent(EMPTY_FORM)).toEqual({
      full_name: "common.required",
      father_name: "common.required",
      date_of_birth: "common.required",
    });
    expect(
      validateStudent({
        ...EMPTY_FORM,
        full_name: "A",
        father_name: "B",
        date_of_birth: "2015-01-01",
      }),
    ).toEqual({});
  });
  it("maps the form to create and patch requests", () => {
    const f = {
      ...EMPTY_FORM,
      full_name: " Ali ",
      father_name: "Ahmed",
      date_of_birth: "2015-03-04",
      cnic: "",
      section_id: "sec1",
      roll_no: "7",
      custom: [
        { key: "house", value: "Red" },
        { key: " ", value: "x" },
      ],
    };
    const req = toCreateRequest(f, [{ relationship: "father", full_name: "Ahmed" }], true);
    expect(req).toMatchObject({
      full_name: "Ali",
      cnic: undefined,
      section_id: "sec1",
      roll_no: 7,
      custom: { house: "Red" },
      force: true,
    });
    expect(req.guardians).toHaveLength(1);
    expect(toPatch(f)).toMatchObject({ full_name: "Ali", cnic: null, custom: { house: "Red" } });
  });
});

describe("Students directory", () => {
  beforeEach(() => {
    vi.mocked(api.students).mockImplementation(async (q) => ({
      items: q?.q
        ? rows.filter((r) => r.full_name.toLowerCase().includes(q.q!.toLowerCase()))
        : rows,
      page: 1,
      page_size: 50,
      total: q?.q ? 1 : 2,
    }));
  });

  it("lists students with class, roll number and status, and searches", async () => {
    renderAt("/students");
    expect(await screen.findByText("Ali Khan")).toBeInTheDocument();
    expect(screen.getByText("Sara Khan")).toBeInTheDocument();
    const row = screen.getByText("Ali Khan").closest("tr")!;
    expect(within(row).getByText("1-A")).toBeInTheDocument();
    expect(within(row).getByText("#7")).toBeInTheDocument();
    expect(within(row).getByText("Active")).toBeInTheDocument();
    expect(screen.getByText("2 results")).toBeInTheDocument();
    await userEvent.type(screen.getByRole("searchbox"), "sara");
    await waitFor(() =>
      expect(api.students).toHaveBeenCalledWith(expect.objectContaining({ q: "sara" })),
    );
    expect(await screen.findByText("1 results")).toBeInTheDocument();
    expect(screen.queryByText("Ali Khan")).not.toBeInTheDocument();
  });

  it("hides the add button without students.manage", async () => {
    renderAt("/students", ["students.read"]);
    await screen.findByText("Ali Khan");
    expect(screen.queryByRole("button", { name: "Add student" })).not.toBeInTheDocument();
  });

  it("sorts by clicking a column header", async () => {
    renderAt("/students");
    await screen.findByText("Ali Khan");
    await userEvent.click(screen.getByRole("button", { name: /Admission date/ }));
    await waitFor(() =>
      expect(api.students).toHaveBeenCalledWith(
        expect.objectContaining({ sort: "admission_date", order: "asc" }),
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: /Admission date/ }));
    await waitFor(() =>
      expect(api.students).toHaveBeenCalledWith(
        expect.objectContaining({ sort: "admission_date", order: "desc" }),
      ),
    );
  });
});

describe("Add student", () => {
  it("validates inline, warns about duplicates, and lets the user save anyway", async () => {
    vi.mocked(api.createStudent)
      .mockImplementationOnce(async () => {
        const err = new ApiError(
          409,
          "possible_duplicate",
          "A student with the same details already exists.",
        );
        (err as ApiError & { details?: unknown }).details = [
          { student: detail, reason: "same_name_dob_father" },
        ];
        throw err;
      })
      .mockImplementationOnce(async () => ({
        ...detail,
        id: "st9",
        student_number: "KHI-MC-2026-00009",
      }));
    vi.mocked(api.student).mockResolvedValue(detail);
    renderAt("/students/new");
    await screen.findByRole("heading", { level: 1, name: "Add student" });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getAllByRole("alert")).toHaveLength(3);
    expect(api.createStudent).not.toHaveBeenCalled();

    await userEvent.type(screen.getByLabelText("Student name"), "Ali Khan");
    await userEvent.type(screen.getByLabelText("Father's name"), "Ahmed Khan");
    await userEvent.type(screen.getByLabelText("Date of birth"), "2015-03-04");
    await userEvent.selectOptions(screen.getByLabelText("Class / section"), "sec1");
    await userEvent.click(screen.getByRole("button", { name: "New guardian" }));
    await userEvent.type(screen.getByLabelText("Name"), "Ahmed Khan");
    await userEvent.type(screen.getByLabelText("Phone"), "0300-1111111");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    const dialog = await screen.findByRole("dialog", { name: "Possible duplicate" });
    expect(
      within(dialog).getByText("Same name, date of birth and father's name"),
    ).toBeInTheDocument();
    expect(api.createStudent).toHaveBeenLastCalledWith(
      expect.objectContaining({
        force: false,
        section_id: "sec1",
        guardians: [
          expect.objectContaining({
            full_name: "Ahmed Khan",
            phone: "0300-1111111",
            relationship: "father",
            is_primary: true,
          }),
        ],
      }),
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Save anyway" }));
    await waitFor(() =>
      expect(api.createStudent).toHaveBeenLastCalledWith(expect.objectContaining({ force: true })),
    );
    expect(await screen.findByRole("heading", { level: 1, name: "Ali Khan" })).toBeInTheDocument();
  });
});

describe("Student profile", () => {
  beforeEach(() => vi.mocked(api.student).mockResolvedValue(detail));

  it("shows details, guardians, siblings and every module tab", async () => {
    renderAt("/students/st1");
    expect(await screen.findByRole("heading", { level: 1, name: "Ali Khan" })).toBeInTheDocument();
    expect(screen.getByText("42101-1234567-1")).toBeInTheDocument();
    expect(screen.getByText("Red")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Sara Khan" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Guardians" }));
    expect(screen.getByText("Ahmed Khan", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByText("Primary contact")).toBeInTheDocument();
    // Fees, Attendance and Results are real tabs now; nothing is "Coming soon".
    expect(screen.getAllByRole("tab")).toHaveLength(5);
    expect(screen.queryByText("Coming soon")).not.toBeInTheDocument();
  });

  it("withdraws a student with a reason", async () => {
    vi.mocked(api.withdrawStudent).mockResolvedValue({
      ...detail,
      status: "left",
      left_on: "2026-06-30",
      left_reason: "Moved",
    });
    renderAt("/students/st1");
    await userEvent.click(await screen.findByRole("button", { name: "Withdraw" }));
    const dialog = screen.getByRole("dialog", { name: "Withdraw student" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Withdraw" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("This field is required.");
    await userEvent.type(within(dialog).getByLabelText("Reason"), "Moved");
    await userEvent.click(within(dialog).getByRole("button", { name: "Withdraw" }));
    await waitFor(() =>
      expect(api.withdrawStudent).toHaveBeenCalledWith(
        "st1",
        expect.objectContaining({ status: "left", reason: "Moved" }),
      ),
    );
    expect(await screen.findByText("Left")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Re-admit" })).toBeInTheDocument();
  });

  it("hides editing actions for read-only users", async () => {
    renderAt("/students/st1", ["students.read"]);
    await screen.findByRole("heading", { level: 1, name: "Ali Khan" });
    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Withdraw" })).not.toBeInTheDocument();
  });
});
