import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import { FeaturesProvider } from "@/app/features";
import type {
  CalendarView,
  DashboardView,
  SheetView,
  StaffDayView,
  StatusView,
  StudentAttendanceView as StudentView,
  UserSummary,
} from "@/api/types";
import { Dashboard } from "@/pages/Dashboard";

const backupOk = {
  last_backup_at: "2026-04-06T02:00:00Z",
  last_backup_file: "/backups/madrasa-khi-mc-20260406-020000.mbk",
  stale: false,
  enabled: true,
  time: "02:00",
  directory: "/backups",
  unattended_ready: true,
  last_error: null,
};
import { MarkTab } from "./tabs/MarkTab";
import { StaffTab } from "./tabs/StaffTab";
import { CalendarTab } from "./tabs/CalendarTab";
import { StudentAttendanceView } from "./StudentAttendanceView";

vi.mock("@/api/endpoints", () => ({
  api: {
    attendanceStatus: vi.fn(),
    attendanceSheet: vi.fn(),
    saveAttendanceSheet: vi.fn(),
    attendanceSheetHistory: vi.fn(),
    staffAttendance: vi.fn(),
    saveStaffAttendance: vi.fn(),
    calendar: vi.fn(),
    setWeekend: vi.fn(),
    createHoliday: vi.fn(),
    deleteHoliday: vi.fn(),
    dashboard: vi.fn(),
    studentAttendance: vi.fn(),
    classes: vi.fn(async () => []),
  },
}));

const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "sana",
  full_name: "Sana Malik",
  email: null,
  is_active: true,
  roles: ["teacher"],
};
const TEACHER = ["attendance.read", "attendance.manage", "academics.read", "teachers.read"];
const ADMIN = [...TEACHER, "attendance.correct", "academics.manage", "teachers.manage"];

const status: StatusView = {
  date: "2026-04-06",
  day_kind: { kind: "school" },
  sections: [
    {
      section_id: "sec1",
      class_id: "c1",
      class_name: "1",
      section_name: "A",
      class_teacher_name: "Sana Malik",
      enrolled: 2,
      sheet_id: null,
      submitted_at: null,
      present: null,
      absent: null,
      late: null,
      leave: null,
    },
    {
      section_id: "sec2",
      class_id: "c2",
      class_name: "2",
      section_name: "A",
      class_teacher_name: null,
      enrolled: 1,
      sheet_id: "sh2",
      submitted_at: "2026-04-06T03:00:00Z",
      present: 1,
      absent: 0,
      late: 0,
      leave: 0,
    },
  ],
  totals: {
    sections: 2,
    marked: 1,
    enrolled: 3,
    present: 1,
    absent: 0,
    late: 0,
    leave: 0,
    percent: 100,
  },
};

const sheet: SheetView = {
  section: {
    id: "sec1",
    class_id: "c1",
    class_name: "1",
    section_name: "A",
    class_teacher_name: "Sana Malik",
  },
  date: "2026-04-06",
  day_kind: { kind: "school" },
  sheet: null,
  marked_by_name: null,
  students: [
    {
      student_id: "ali",
      student_number: "KHI-MC-2026-00001",
      full_name: "Ali Khan",
      father_name: "Ahmed",
      roll_no: 1,
      status: null,
      note: null,
    },
    {
      student_id: "sara",
      student_number: "KHI-MC-2026-00002",
      full_name: "Sara Khan",
      father_name: "Ahmed",
      roll_no: 2,
      status: null,
      note: null,
    },
  ],
  can_edit: true,
  is_correction: false,
};

function renderMark(permissions = TEACHER) {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions }}>
      <MarkTab />
    </SessionProvider>,
  );
}

beforeEach(() => {
  vi.mocked(api.attendanceStatus).mockResolvedValue(status);
  vi.mocked(api.attendanceSheet).mockResolvedValue(sheet);
  vi.mocked(api.saveAttendanceSheet).mockClear();
  vi.mocked(api.saveAttendanceSheet).mockImplementation(async (body) => ({
    ...sheet,
    sheet: {
      id: "sh1",
      section_id: "sec1",
      academic_year_id: "y",
      date: body.date,
      marked_by: "u1",
      submitted_at: "2026-04-06T03:10:00Z",
      present: 2 - body.entries.length,
      absent: body.entries.filter((e) => e.status === "absent").length,
      late: body.entries.filter((e) => e.status === "late").length,
      leave: 0,
    },
    marked_by_name: "Sana Malik",
    is_correction: true,
    can_edit: false,
    students: sheet.students.map((s) => ({
      ...s,
      status: body.entries.find((e) => e.student_id === s.student_id)?.status ?? "present",
    })),
  }));
});

describe("MarkTab", () => {
  it("marks everyone present by default and submits only the exceptions, keyboard first", async () => {
    renderMark();
    const aliRow = await screen.findByTestId("row-ali");
    expect(screen.getByTestId("mark-summary")).toHaveTextContent(
      "2 present · 0 absent · 0 late · 0 leave",
    );
    expect(screen.getByText("Everyone is present unless you mark otherwise.")).toBeInTheDocument();
    const aliGroup = within(aliRow).getByRole("radiogroup");
    expect(within(aliGroup).getByRole("radio", { name: "Present" })).toHaveAttribute(
      "aria-checked",
      "true",
    );

    aliRow.focus();
    await userEvent.keyboard("a");
    expect(within(aliGroup).getByRole("radio", { name: "Absent" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByLabelText("Ali Khan: Note")).toBeInTheDocument();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByTestId("row-sara")).toHaveFocus();
    await userEvent.keyboard("l");
    expect(screen.getByTestId("mark-summary")).toHaveTextContent(
      "0 present · 1 absent · 1 late · 0 leave",
    );
    await userEvent.keyboard("{ArrowUp}");
    expect(aliRow).toHaveFocus();
    await userEvent.type(screen.getByLabelText("Ali Khan: Note"), "sick");

    await userEvent.click(screen.getByRole("button", { name: "Submit attendance" }));
    await waitFor(() =>
      expect(api.saveAttendanceSheet).toHaveBeenCalledWith({
        section_id: "sec1",
        date: expect.any(String),
        entries: [
          { student_id: "ali", status: "absent", note: "sick" },
          { student_id: "sara", status: "late", note: undefined },
        ],
      }),
    );
    expect(await screen.findByText("Attendance saved.")).toBeInTheDocument();
    // Teacher cannot change a submitted sheet.
    expect(screen.queryByRole("button", { name: "Submit attendance" })).not.toBeInTheDocument();
    expect(
      screen.getByText("This day is submitted. Only an administrator can correct it."),
    ).toBeInTheDocument();
  });

  it("lets an admin correct a submitted sheet and shows the history", async () => {
    vi.mocked(api.attendanceSheet).mockResolvedValue({
      ...sheet,
      sheet: {
        id: "sh1",
        section_id: "sec1",
        academic_year_id: "y",
        date: "2026-04-06",
        marked_by: "u1",
        submitted_at: "2026-04-06T03:10:00Z",
        present: 1,
        absent: 1,
        late: 0,
        leave: 0,
      },
      marked_by_name: "Sana Malik",
      students: [
        { ...sheet.students[0]!, status: "absent", note: "sick" },
        { ...sheet.students[1]!, status: "present" },
      ],
      can_edit: true,
      is_correction: true,
    });
    vi.mocked(api.attendanceSheetHistory).mockResolvedValue([
      {
        at: "2026-04-06T03:10:00Z",
        actor_name: "Sana Malik",
        action: "create",
        entity: "attendance_sheet",
        student_name: null,
        old_value: null,
        new_value: { present: 1, absent: 1 },
      },
      {
        at: "2026-04-06T05:00:00Z",
        actor_name: "Ayesha Khan",
        action: "update",
        entity: "attendance_entry",
        student_name: "Ali Khan",
        old_value: { status: "absent" },
        new_value: { status: "present" },
      },
    ]);
    renderMark(ADMIN);
    expect(await screen.findByRole("button", { name: "Save correction" })).toBeInTheDocument();
    expect(screen.getByRole("note")).toHaveTextContent("saved as corrections");
    await userEvent.click(screen.getByRole("button", { name: "History" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Submitted the sheet")).toBeInTheDocument();
    expect(within(dialog).getByText("Ali Khan: Absent → Present")).toBeInTheDocument();
  });

  it("explains weekends and holidays and offers no submit button", async () => {
    vi.mocked(api.attendanceSheet).mockResolvedValue({
      ...sheet,
      day_kind: { kind: "holiday", name: "Eid" },
      can_edit: false,
    });
    renderMark();
    expect(
      await screen.findByText("Holiday: Eid. No attendance is taken on this day."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Submit attendance" })).not.toBeInTheDocument();
  });
});

describe("StaffTab", () => {
  it("saves staff exceptions", async () => {
    const view: StaffDayView = {
      date: "2026-04-06",
      day_kind: { kind: "school" },
      can_edit: true,
      rows: [
        {
          staff_id: "t1",
          staff_number: "S-0001",
          full_name: "Hira",
          designation: "Teacher",
          status: null,
          note: null,
        },
      ],
    };
    vi.mocked(api.staffAttendance).mockResolvedValue(view);
    vi.mocked(api.saveStaffAttendance).mockResolvedValue(view);
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <StaffTab />
      </SessionProvider>,
    );
    await userEvent.selectOptions(await screen.findByLabelText("Hira: Status"), "leave");
    await userEvent.type(screen.getByLabelText("Hira: Note"), "casual");
    await userEvent.click(screen.getByRole("button", { name: "Save staff attendance" }));
    await waitFor(() =>
      expect(api.saveStaffAttendance).toHaveBeenCalledWith({
        date: expect.any(String),
        entries: [{ staff_id: "t1", status: "leave", note: "casual" }],
      }),
    );
  });
});

describe("CalendarTab", () => {
  it("edits weekend days and adds a holiday", async () => {
    const cal: CalendarView = { weekend_days: [7], holidays: [] };
    vi.mocked(api.calendar).mockResolvedValue(cal);
    vi.mocked(api.setWeekend).mockImplementation(async (days) => ({ ...cal, weekend_days: days }));
    vi.mocked(api.createHoliday).mockResolvedValue({
      id: "h1",
      school_id: "s",
      name: "Eid",
      starts_on: "2026-05-27",
      ends_on: "2026-05-30",
    });
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <CalendarTab />
      </SessionProvider>,
    );
    expect(await screen.findByLabelText("Sunday")).toBeChecked();
    await userEvent.click(screen.getByLabelText("Saturday"));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.setWeekend).toHaveBeenCalledWith([6, 7]));

    await userEvent.type(screen.getByLabelText("Name"), "Eid");
    await userEvent.type(screen.getByLabelText("From"), "2026-05-27");
    await userEvent.type(screen.getByLabelText(/^To/), "2026-05-30");
    await userEvent.click(screen.getByRole("button", { name: "Add holiday" }));
    await waitFor(() =>
      expect(api.createHoliday).toHaveBeenCalledWith({
        name: "Eid",
        starts_on: "2026-05-27",
        ends_on: "2026-05-30",
      }),
    );
  });
});

describe("Dashboard attendance card", () => {
  it("shows today's percentage and the sections not yet marked", async () => {
    const dash: DashboardView = {
      date: "2026-04-06",
      day_kind: { kind: "school" },
      attendance: {
        totals: status.totals,
        unmarked: [
          { section_id: "sec1", class_name: "1", section_name: "A", class_teacher_name: "Sana" },
        ],
      },
      backup: backupOk,
    };
    vi.mocked(api.dashboard).mockResolvedValue(dash);
    renderWithProviders(
      <FeaturesProvider initial={{ edition: "lite", features: ["dashboard", "attendance"] }}>
        <SessionProvider initial={{ user, permissions: TEACHER }}>
          <Routes>
            <Route path="/" element={<Dashboard />} />
          </Routes>
        </SessionProvider>
      </FeaturesProvider>,
    );
    expect(await screen.findByText("100.0%")).toBeInTheDocument();
    expect(screen.getByText("1 of 2 sections marked")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /1-A/ })).toHaveAttribute(
      "href",
      "/attendance?tab=mark&section=sec1",
    );
  });

  it("says when there is no school today", async () => {
    vi.mocked(api.dashboard).mockResolvedValue({
      date: "2026-04-05",
      day_kind: { kind: "weekend" },
      attendance: { totals: { ...status.totals, percent: null, marked: 0 }, unmarked: [] },
      backup: { ...backupOk, stale: true, last_backup_at: null, last_backup_file: null },
    });
    renderWithProviders(
      <FeaturesProvider initial={{ edition: "lite", features: ["dashboard", "attendance"] }}>
        <SessionProvider initial={{ user, permissions: TEACHER }}>
          <Dashboard />
        </SessionProvider>
      </FeaturesProvider>,
    );
    expect(await screen.findByText("No school today: weekend")).toBeInTheDocument();
    expect(screen.getByText("The last backup is older than 2 days.")).toBeInTheDocument();
  });
});

describe("StudentAttendanceView", () => {
  it("shows the summary and the days", async () => {
    const v: StudentView = {
      student_id: "ali",
      from: "2026-04-01",
      to: "2026-04-30",
      summary: { present: 18, absent: 2, late: 1, leave: 0 },
      percent: 90.5,
      days: [
        { date: "2026-04-07", status: "absent", note: "sick", section_id: "sec1" },
        { date: "2026-04-06", status: "present", note: null, section_id: "sec1" },
      ],
    };
    vi.mocked(api.studentAttendance).mockResolvedValue(v);
    renderWithProviders(<StudentAttendanceView studentId="ali" />);
    expect(await screen.findByText("90.5%")).toBeInTheDocument();
    expect(screen.getByText("21 days marked")).toBeInTheDocument();
    expect(screen.getByText("sick")).toBeInTheDocument();
    expect(screen.getAllByText("Absent")).toHaveLength(2);
  });
});
