import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type {
  LeaveRequestView,
  LeaveRequestsView,
  StaffRegisterView,
  StudentLeaveList,
  UserSummary,
} from "@/api/types";
import { LeavePage } from "./LeavePage";

vi.mock("@/api/endpoints", () => ({
  api: {
    leaveRequests: vi.fn(),
    leaveTypes: vi.fn(),
    staff: vi.fn(),
    decideLeaveRequest: vi.fn(),
    withdrawLeaveRequest: vi.fn(),
    createLeaveRequest: vi.fn(),
    studentLeave: vi.fn(),
    decideStudentLeave: vi.fn(),
    staffRegister: vi.fn(),
    academicYears: vi.fn(),
    leaveBalances: vi.fn(),
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
const teacher: UserSummary = {
  ...admin,
  id: "u2",
  username: "sana",
  full_name: "Sana Malik",
  roles: ["teacher"],
};

const request = (over: Partial<LeaveRequestView>): LeaveRequestView => ({
  id: "r1",
  school_id: "s",
  staff_id: "t1",
  leave_type_id: "lt1",
  academic_year_id: "y",
  from_date: "2026-04-06",
  to_date: "2026-04-08",
  half_day: null,
  days: "3.0",
  reason: "Family",
  attachment_note: "",
  status: "pending",
  requested_by: "u2",
  decided_by: null,
  decided_at: null,
  decision_note: "",
  created_at: "2026-04-01T00:00:00Z",
  hlc: "",
  staff_name: "Sana Malik",
  staff_number: "S-001",
  type_name: "Annual",
  decided_by_name: null,
  ...over,
});

const students: StudentLeaveList = {
  requests: [
    {
      id: "sl1",
      school_id: "s",
      student_id: "st1",
      academic_year_id: "y",
      from_date: "2026-04-06",
      to_date: "2026-04-07",
      reason: "Wedding",
      guardian_id: null,
      status: "pending",
      recorded_by: "u1",
      decided_by: null,
      decided_at: null,
      decision_note: "",
      created_at: "2026-04-01T00:00:00Z",
      hlc: "",
      student_number: "2026-0001",
      full_name: "Ali Khan",
      class_name: "9",
      section_id: "sec1",
      section_name: "A",
      guardian_name: null,
      decided_by_name: null,
      days: 2,
    },
  ],
  decide_sections: ["sec1"],
  can_manage: false,
};

const register: StaffRegisterView = {
  month: "2026-04-01",
  days: ["2026-04-01", "2026-04-02", "2026-04-03"],
  school_days: [true, true, false],
  rows: [
    {
      staff_id: "t1",
      staff_number: "S-001",
      full_name: "Sana Malik",
      designation: "Teacher",
      cells: [
        { status: "present", arrived_at: null, approved_leave: false },
        { status: "late", arrived_at: "08:30:00", approved_leave: false },
        null,
      ],
      present: 1,
      absent: 0,
      late: 1,
      leave: 0,
    },
  ],
};

function renderPage(user: UserSummary, permissions: string[]) {
  return renderWithProviders(
    <FeaturesProvider initial={{ edition: "standard", features: ["dashboard", "leave"] }}>
      <SessionProvider initial={{ user, permissions }}>
        <LeavePage />
      </SessionProvider>
    </FeaturesProvider>,
    { route: "/leave" },
  );
}

describe("LeavePage", () => {
  beforeEach(() => {
    vi.mocked(api.leaveTypes).mockResolvedValue([]);
    vi.mocked(api.staff).mockResolvedValue([]);
    vi.mocked(api.studentLeave).mockResolvedValue(students);
    vi.mocked(api.staffRegister).mockResolvedValue(register);
    vi.mocked(api.academicYears).mockResolvedValue([]);
    vi.mocked(api.leaveBalances).mockResolvedValue([]);
  });

  it("lets an approver decide a pending request and hides the types tab from teachers", async () => {
    const view: LeaveRequestsView = {
      requests: [request({})],
      can_decide: true,
      own_staff_id: null,
    };
    vi.mocked(api.leaveRequests).mockResolvedValue(view);
    vi.mocked(api.decideLeaveRequest).mockResolvedValue({ ...request({}), status: "approved" });
    renderPage(admin, ["leave.read", "leave.manage", "leave.approve"]);
    expect(await screen.findByText("Sana Malik")).toBeInTheDocument();
    expect(screen.getAllByText("Pending").length).toBeGreaterThan(0);
    expect(screen.getAllByRole("tab")).toHaveLength(5);
    await userEvent.click(screen.getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(api.decideLeaveRequest).toHaveBeenCalledWith("r1", "approved"));
  });

  it("shows a teacher only her own requests with a withdraw button and no decisions", async () => {
    const view: LeaveRequestsView = {
      requests: [request({ status: "approved", decided_by_name: "Ayesha Khan" })],
      can_decide: false,
      own_staff_id: "t1",
    };
    vi.mocked(api.leaveRequests).mockResolvedValue(view);
    renderPage(teacher, ["leave.read"]);
    expect(await screen.findByText("Approved")).toBeInTheDocument();
    expect(screen.getAllByRole("tab")).toHaveLength(4);
    expect(screen.queryByRole("button", { name: "Approve" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Withdraw" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Request leave" })).toBeInTheDocument();
  });

  it("lets a class teacher approve student leave for her section", async () => {
    vi.mocked(api.leaveRequests).mockResolvedValue({
      requests: [],
      can_decide: false,
      own_staff_id: "t1",
    });
    vi.mocked(api.decideStudentLeave).mockResolvedValue({
      ...students.requests[0]!,
      status: "approved",
    });
    renderPage(teacher, ["leave.read"]);
    await userEvent.click(await screen.findByRole("tab", { name: "Student leave" }));
    expect(await screen.findByText("Ali Khan")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(api.decideStudentLeave).toHaveBeenCalledWith("sl1", "approved"));
  });

  it("draws the staff register with late times and shaded weekends", async () => {
    vi.mocked(api.leaveRequests).mockResolvedValue({
      requests: [],
      can_decide: false,
      own_staff_id: null,
    });
    renderPage(admin, ["leave.read", "leave.manage"]);
    await userEvent.click(await screen.findByRole("tab", { name: "Staff register" }));
    const table = await screen.findByTestId("staff-register");
    expect(within(table).getByText("08:30")).toBeInTheDocument();
    expect(within(table).getAllByText("P")).toHaveLength(2);
    expect(within(table).getAllByText("L")).toHaveLength(2);
  });
});
