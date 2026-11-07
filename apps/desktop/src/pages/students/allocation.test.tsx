import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { SectionAllocations, StudentSubjects, UserSummary } from "@/api/types";
import { SubjectAllocationPage } from "./SubjectAllocationPage";
import { StudentSubjectsTab } from "./StudentSubjectsTab";
import { SubjectGroupsCard } from "../academics/SubjectGroupsCard";

vi.mock("@/api/endpoints", () => ({
  api: {
    academicYears: vi.fn(async () => [
      {
        id: "y",
        school_id: "s",
        name: "2026-27",
        start_date: "2026-04-01",
        end_date: "2027-03-31",
        start_month: 4,
        is_active: true,
      },
    ]),
    classes: vi.fn(async () => [{ id: "c9", school_id: "s", name: "9", level: 9 }]),
    sections: vi.fn(async () => [
      {
        id: "s9a",
        school_id: "s",
        class_id: "c9",
        academic_year_id: "y",
        name: "A",
        class_teacher_id: null,
        capacity: null,
        class_name: "9",
        class_level: 9,
        class_teacher_name: null,
        student_count: 2,
      },
    ]),
    sectionAllocations: vi.fn(),
    setSectionAllocations: vi.fn(),
    studentSubjects: vi.fn(),
    setStudentSubjects: vi.fn(),
    classSubjects: vi.fn(),
    subjectGroups: vi.fn(),
    createSubjectGroup: vi.fn(),
    updateSubjectGroup: vi.fn(),
    deleteSubjectGroup: vi.fn(),
    downloadAllocationTemplate: vi.fn(async () => undefined),
    parseAllocations: vi.fn(),
    previewAllocations: vi.fn(),
    applyAllocations: vi.fn(),
    rollbackImport: vi.fn(),
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
const ADMIN = ["students.read", "students.manage", "academics.read", "academics.manage"];

const groups = [
  {
    id: "g1",
    school_id: "s",
    class_id: "c9",
    name: "Pre-Medical",
    position: 0,
    hlc: "",
    subject_ids: ["phy", "che", "bio"],
  },
  {
    id: "g2",
    school_id: "s",
    class_id: "c9",
    name: "Computer Science",
    position: 1,
    hlc: "",
    subject_ids: ["phy", "che", "cs"],
  },
];
const subjects = [
  { subject_id: "eng", name: "English", code: "ENG", is_elective: false },
  { subject_id: "phy", name: "Physics", code: "PHY", is_elective: true },
  { subject_id: "che", name: "Chemistry", code: "CHE", is_elective: true },
  { subject_id: "bio", name: "Biology", code: "BIO", is_elective: true },
  { subject_id: "cs", name: "Computer", code: "CS", is_elective: true },
];
const view: SectionAllocations = {
  section_id: "s9a",
  class_id: "c9",
  class_name: "9",
  section_name: "A",
  subjects,
  groups,
  students: [
    {
      enrollment_id: "e1",
      student_id: "ali",
      student_number: "KHI-MC-2026-00001",
      full_name: "Ali Khan",
      roll_no: 1,
      group_id: "g1",
      allocated: true,
      allocated_ids: ["phy", "che", "bio"],
      subject_ids: ["eng", "phy", "che", "bio"],
    },
    {
      enrollment_id: "e2",
      student_id: "sara",
      student_number: "KHI-MC-2026-00002",
      full_name: "Sara Khan",
      roll_no: 2,
      group_id: null,
      allocated: false,
      allocated_ids: [],
      subject_ids: ["eng", "phy", "che", "bio", "cs"],
    },
  ],
};

beforeEach(() => {
  vi.mocked(api.sectionAllocations).mockResolvedValue(view);
  vi.mocked(api.setSectionAllocations).mockReset();
  vi.mocked(api.setSectionAllocations).mockResolvedValue(view);
});

describe("SubjectAllocationPage", () => {
  it("shows groups and electives per student and saves the changed rows", async () => {
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <SubjectAllocationPage />
      </SessionProvider>,
      { route: "/students/subjects" },
    );
    await screen.findByRole("option", { name: "9 A" });
    await userEvent.selectOptions(screen.getByLabelText("Section"), "s9a");
    const table = await screen.findByRole("table", { name: "Subject allocation" });
    const ali = within(table).getByText("Ali Khan").closest("tr")!;
    expect(within(ali).getByLabelText("Ali Khan: Group")).toHaveValue("g1");
    expect(within(ali).getByLabelText("Ali Khan: Biology")).toBeChecked();
    expect(within(ali).getByLabelText("Ali Khan: Computer")).not.toBeChecked();
    expect(within(ali).getByText("Allocated")).toBeInTheDocument();
    const sara = within(table).getByText("Sara Khan").closest("tr")!;
    expect(within(sara).getByText("All subjects")).toBeInTheDocument();
    // Unallocated students show every elective as taken.
    expect(within(sara).getByLabelText("Sara Khan: Computer")).toBeChecked();
    const save = screen.getByRole("button", { name: "Save allocations" });
    expect(save).toBeDisabled();
    await userEvent.selectOptions(within(sara).getByLabelText("Sara Khan: Group"), "g2");
    expect(within(sara).getByLabelText("Sara Khan: Biology")).not.toBeChecked();
    await userEvent.click(within(sara).getByLabelText("Sara Khan: Biology"));
    await userEvent.click(save);
    await waitFor(() =>
      expect(api.setSectionAllocations).toHaveBeenCalledWith("s9a", [
        { enrollment_id: "e2", group_id: "g2", subject_ids: ["phy", "che", "cs", "bio"] },
      ]),
    );
  });

  it("explains when the class has no electives", async () => {
    vi.mocked(api.sectionAllocations).mockResolvedValue({
      ...view,
      subjects: subjects.map((s) => ({ ...s, is_elective: false })),
      groups: [],
    });
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <SubjectAllocationPage />
      </SessionProvider>,
      { route: "/students/subjects" },
    );
    await screen.findByRole("option", { name: "9 A" });
    await userEvent.selectOptions(screen.getByLabelText("Section"), "s9a");
    expect(await screen.findByText(/This class has no electives/)).toBeInTheDocument();
  });
});

describe("StudentSubjectsTab", () => {
  it("lists compulsory subjects, picks a group and saves", async () => {
    const mine: StudentSubjects = {
      enrollment_id: "e2",
      academic_year_id: "y",
      class_id: "c9",
      class_name: "9",
      group_id: null,
      allocated: false,
      subjects: subjects.map((s) => ({ ...s, taken: true })),
      groups,
    };
    vi.mocked(api.studentSubjects).mockResolvedValue(mine);
    vi.mocked(api.setStudentSubjects).mockResolvedValue({
      ...mine,
      group_id: "g1",
      allocated: true,
    });
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <StudentSubjectsTab studentId="sara" manage />
      </SessionProvider>,
    );
    expect(await screen.findByText("English")).toBeInTheDocument();
    expect(screen.getByText("All subjects")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Group"), "g1");
    expect(screen.getByLabelText("Biology")).toBeChecked();
    expect(screen.getByLabelText("Computer")).not.toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "Save allocations" }));
    await waitFor(() =>
      expect(api.setStudentSubjects).toHaveBeenCalledWith("sara", {
        academic_year_id: "y",
        group_id: "g1",
        subject_ids: ["phy", "che", "bio"],
      }),
    );
  });
});

describe("SubjectGroupsCard", () => {
  it("lists groups and creates one from the class electives", async () => {
    vi.mocked(api.classSubjects).mockResolvedValue({
      subject_ids: ["eng", "phy", "che", "bio", "cs"],
      elective_ids: ["phy", "che", "bio", "cs"],
    });
    vi.mocked(api.subjectGroups).mockResolvedValue(groups);
    vi.mocked(api.createSubjectGroup).mockResolvedValue(groups[0]!);
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <SubjectGroupsCard
          classes={[{ id: "c9", school_id: "s", name: "9", level: 9 }]}
          subjects={subjects.map((s) => ({
            id: s.subject_id,
            school_id: "s",
            name: s.name,
            code: s.code,
          }))}
          manage
          onError={() => undefined}
        />
      </SessionProvider>,
    );
    const table = await screen.findByRole("table", { name: "Subject groups" });
    expect(within(table).getByText("Pre-Medical")).toBeInTheDocument();
    expect(within(table).getByText("Physics, Chemistry, Biology")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "New group" }));
    const editor = screen.getByTestId("group-editor");
    await userEvent.type(within(editor).getByLabelText("Group"), "Arts");
    await userEvent.click(within(editor).getByLabelText("Computer"));
    await userEvent.click(within(editor).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(api.createSubjectGroup).toHaveBeenCalledWith("c9", {
        name: "Arts",
        position: 2,
        subject_ids: ["cs"],
      }),
    );
  });
});
