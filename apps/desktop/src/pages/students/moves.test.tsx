import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { ClassSections, TeacherGrid, UserSummary } from "@/api/types";
import { SectionMovesPage } from "./SectionMovesPage";
import { TeacherAllocationPage } from "../staff/TeacherAllocationPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    classes: vi.fn(async () => [{ id: "c1", school_id: "s", name: "1", level: 1 }]),
    classSections: vi.fn(),
    balanceSections: vi.fn(),
    moveStudents: vi.fn(),
    teacherGrid: vi.fn(),
    setTeacherCell: vi.fn(),
    updateSection: vi.fn(),
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
const ADMIN = ["students.read", "students.manage", "teachers.read", "teachers.manage"];

const student = (n: number, section: string, gender: "male" | "female") => ({
  student_id: `st${n}`,
  enrollment_id: `e${n}`,
  section_id: section,
  student_number: `KHI-MC-2026-0000${n}`,
  full_name: `Student ${n}`,
  gender,
  roll_no: n,
});
const view: ClassSections = {
  academic_year_id: "y",
  class_id: "c1",
  class_name: "1",
  sections: [
    {
      section_id: "sa",
      name: "A",
      capacity: 35,
      class_teacher_name: "Sana Malik",
      size: 3,
      male: 2,
      female: 1,
      other: 0,
      students: [student(1, "sa", "male"), student(2, "sa", "female"), student(3, "sa", "male")],
    },
    {
      section_id: "sb",
      name: "B",
      capacity: null,
      class_teacher_name: null,
      size: 1,
      male: 0,
      female: 1,
      other: 0,
      students: [student(4, "sb", "female")],
    },
  ],
};

describe("SectionMovesPage", () => {
  beforeEach(() => {
    vi.mocked(api.classSections).mockResolvedValue(view);
    vi.mocked(api.moveStudents).mockReset();
    vi.mocked(api.moveStudents).mockResolvedValue({ moved: 1, view });
  });

  it("moves ticked students to the chosen section with the roll-number choice", async () => {
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <SectionMovesPage />
      </SessionProvider>,
      { route: "/students/sections" },
    );
    await userEvent.selectOptions(await screen.findByLabelText("Class"), "c1");
    const a = await screen.findByRole("table", { name: "1 A" });
    expect(screen.getByText("3 students · 2 boys · 1 girls · Capacity 35")).toBeInTheDocument();
    await userEvent.click(within(a).getByLabelText("Student 3"));
    const move = screen.getByRole("button", { name: "Move 1 students" });
    expect(move).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText("Move to"), "sb");
    await userEvent.selectOptions(screen.getByLabelText("Roll numbers"), "keep");
    await userEvent.click(move);
    await waitFor(() =>
      expect(api.moveStudents).toHaveBeenCalledWith({
        class_id: "c1",
        academic_year_id: "y",
        moves: [{ student_id: "st3", to_section_id: "sb" }],
        roll_numbers: "keep",
      }),
    );
  });

  it("previews the balancing proposal and applies it", async () => {
    vi.mocked(api.balanceSections).mockResolvedValue({
      moves: [
        {
          student_id: "st3",
          full_name: "Student 3",
          student_number: "KHI-MC-2026-00003",
          roll_no: 3,
          from_section_id: "sa",
          from_name: "A",
          to_section_id: "sb",
          to_name: "B",
        },
      ],
      sizes: [
        { section_id: "sa", name: "A", before: 3, after: 2 },
        { section_id: "sb", name: "B", before: 1, after: 2 },
      ],
    });
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <SectionMovesPage />
      </SessionProvider>,
      { route: "/students/sections" },
    );
    await userEvent.selectOptions(await screen.findByLabelText("Class"), "c1");
    await screen.findByRole("table", { name: "1 A" });
    await userEvent.click(screen.getByLabelText("Student 1"));
    await userEvent.click(screen.getByRole("button", { name: "Balance sections" }));
    await waitFor(() =>
      expect(api.balanceSections).toHaveBeenCalledWith({
        class_id: "c1",
        academic_year_id: "y",
        exclude_student_ids: ["st1"],
      }),
    );
    const dialog = await screen.findByTestId("balance-preview");
    expect(within(dialog).getByText("Student 3")).toBeInTheDocument();
    expect(within(dialog).getByText("A: 3 → 2")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Apply moves" }));
    await waitFor(() =>
      expect(api.moveStudents).toHaveBeenCalledWith({
        class_id: "c1",
        academic_year_id: "y",
        moves: [{ student_id: "st3", to_section_id: "sb" }],
        roll_numbers: "reassign",
      }),
    );
  });
});

describe("TeacherAllocationPage", () => {
  it("shows the grid and saves a cell and the class teacher at once", async () => {
    const grid: TeacherGrid = {
      academic_year_id: "y",
      subjects: [
        { id: "eng", name: "English", code: "ENG" },
        { id: "mth", name: "Mathematics", code: "MTH" },
      ],
      sections: [
        {
          section_id: "sa",
          class_id: "c1",
          class_name: "1",
          section_name: "A",
          class_teacher_id: "t1",
          class_teacher_name: "Sana Malik",
          subject_ids: ["eng", "mth"],
          cells: [
            { subject_id: "eng", staff_id: "t1", staff_name: "Sana Malik", extra: 0 },
            { subject_id: "mth", staff_id: null, staff_name: null, extra: 0 },
          ],
        },
      ],
      staff: [
        {
          id: "t1",
          full_name: "Sana Malik",
          designation: "Teacher",
          cells: 1,
          class_teacher_of: 1,
        },
        { id: "t2", full_name: "Bilal Ahmed", designation: "", cells: 0, class_teacher_of: 0 },
      ],
    };
    vi.mocked(api.teacherGrid).mockResolvedValue(grid);
    vi.mocked(api.setTeacherCell).mockResolvedValue(undefined);
    vi.mocked(api.updateSection).mockResolvedValue({} as never);
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <TeacherAllocationPage />
      </SessionProvider>,
      { route: "/teachers/allocation" },
    );
    const table = await screen.findByRole("table", { name: "Teacher allocation" });
    expect(within(table).getByLabelText("1 A: English")).toHaveValue("t1");
    expect(within(table).getByLabelText("1 A: Mathematics")).toHaveValue("");
    await userEvent.selectOptions(within(table).getByLabelText("1 A: Mathematics"), "t2");
    await waitFor(() =>
      expect(api.setTeacherCell).toHaveBeenCalledWith({
        section_id: "sa",
        subject_id: "mth",
        staff_id: "t2",
      }),
    );
    await userEvent.selectOptions(within(table).getByLabelText("1 A: Class teacher"), "t2");
    await waitFor(() =>
      expect(api.updateSection).toHaveBeenCalledWith("sa", { class_teacher_id: "t2" }),
    );
    const load = screen.getByRole("list", { name: "Teaching load" });
    expect(within(load).getByText("1 cells · class teacher of 1")).toBeInTheDocument();
  });
});
