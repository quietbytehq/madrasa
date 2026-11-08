import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type { RouteDetail, RouteView, TransportStudent, UserSummary } from "@/api/types";
import { TransportPage } from "./TransportPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    transportRoutes: vi.fn(),
    transportRoute: vi.fn(),
    createTransportRoute: vi.fn(),
    updateTransportRoute: vi.fn(),
    deleteTransportRoute: vi.fn(),
    vehicles: vi.fn(),
    academicYears: vi.fn(),
    sections: vi.fn(),
    transportStudents: vi.fn(),
    assignTransportBulk: vi.fn(),
    updateTransportAssignment: vi.fn(),
    transportRoster: vi.fn(),
    transportSettings: vi.fn(),
    updateTransportSettings: vi.fn(),
    feeHeads: vi.fn(),
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

const north: RouteView = {
  id: "r1",
  school_id: "s",
  name: "North",
  code: "N1",
  vehicle_id: "v1",
  monthly_fee: "1500.00",
  active: true,
  notes: "",
  hlc: "",
  vehicle_registration: "KHI-1234",
  vehicle_capacity: 2,
  driver_name: "Rashid",
  stops: 2,
  students: 1,
};

const northDetail: RouteDetail = {
  ...north,
  stop_list: [
    {
      id: "st1",
      school_id: "s",
      route_id: "r1",
      name: "Gulshan",
      position: 0,
      pickup_time: "07:10:00",
      drop_time: null,
      monthly_fee: null,
      hlc: "",
    },
    {
      id: "st2",
      school_id: "s",
      route_id: "r1",
      name: "NIPA",
      position: 1,
      pickup_time: "07:25:00",
      drop_time: null,
      monthly_fee: "1200.00",
      hlc: "",
    },
  ],
};

const student = (over: Partial<TransportStudent>): TransportStudent => ({
  student_id: "s1",
  student_number: "2026-0001",
  full_name: "Ali Khan",
  class_name: "9",
  section_name: "A",
  section_id: "sec1",
  roll_no: 1,
  assignment_id: null,
  route_id: null,
  route_name: null,
  stop_id: null,
  stop_name: null,
  charge: null,
  ...over,
});

function renderPage(permissions: string[]) {
  return renderWithProviders(
    <FeaturesProvider
      initial={{ edition: "standard", features: ["dashboard", "students", "transport"] }}
    >
      <SessionProvider initial={{ user: admin, permissions }}>
        <TransportPage />
      </SessionProvider>
    </FeaturesProvider>,
    { route: "/transport" },
  );
}

describe("TransportPage", () => {
  beforeEach(() => {
    vi.mocked(api.transportRoutes).mockResolvedValue([north]);
    vi.mocked(api.transportRoute).mockResolvedValue(northDetail);
    vi.mocked(api.vehicles).mockResolvedValue([]);
    vi.mocked(api.academicYears).mockResolvedValue([
      {
        id: "y1",
        school_id: "s",
        name: "2026-27",
        start_date: "2026-04-01",
        end_date: "2027-03-31",
        start_month: 4,
        is_active: true,
      },
    ]);
    vi.mocked(api.sections).mockResolvedValue([]);
  });

  it("lists routes and opens one with its stops", async () => {
    renderPage(["transport.read"]);
    expect(screen.getAllByRole("tab")).toHaveLength(4);
    expect(await screen.findByText("North")).toBeInTheDocument();
    expect(screen.queryByTestId("add-route")).not.toBeInTheDocument();
    await userEvent.click(screen.getByText("North"));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByDisplayValue("NIPA")).toBeInTheDocument();
    expect(within(dialog).queryByTestId("save-route")).not.toBeInTheDocument();
  });

  it("assigns ticked students to a stop and warns when the vehicle is over capacity", async () => {
    vi.mocked(api.transportStudents).mockResolvedValue([
      student({}),
      student({ student_id: "s2", student_number: "2026-0002", full_name: "Sara Noor" }),
      student({
        student_id: "s3",
        student_number: "2026-0003",
        full_name: "Omar Shah",
        assignment_id: "a3",
        route_id: "r1",
        route_name: "North",
        stop_id: "st1",
        stop_name: "Gulshan",
        charge: "1500.00",
      }),
    ]);
    vi.mocked(api.assignTransportBulk).mockResolvedValue({
      assigned: 2,
      refused: [],
      route_students: 3,
      seats_left: -1,
      warning: { kind: "over", by: 1 },
    });
    renderPage(["transport.read", "transport.manage"]);
    await userEvent.click(screen.getByRole("tab", { name: "Assign" }));
    expect(await screen.findByText("Sara Noor")).toBeInTheDocument();
    expect(screen.getAllByText("No transport")).toHaveLength(2);
    await userEvent.click(screen.getByRole("checkbox", { name: "Ali Khan" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "Sara Noor" }));
    const bar = screen.getByTestId("assign-bar");
    await userEvent.selectOptions(within(bar).getByLabelText("Route"), "r1");
    await userEvent.selectOptions(await within(bar).findByLabelText("Stop"), "st2");
    await userEvent.click(screen.getByTestId("assign"));
    await waitFor(() =>
      expect(api.assignTransportBulk).toHaveBeenCalledWith(
        expect.objectContaining({
          student_ids: ["s1", "s2"],
          stop_id: "st2",
          academic_year_id: "y1",
        }),
      ),
    );
    expect(await screen.findByText("Over capacity by 1")).toBeInTheDocument();
  });

  it("shows the roster by stop with the capacity state", async () => {
    vi.mocked(api.transportRoster).mockResolvedValue({
      ...north,
      academic_year_id: "y1",
      total: 1,
      seats_left: 1,
      warning: null,
      by_stop: [
        {
          stop: northDetail.stop_list[0]!,
          students: [
            {
              id: "a1",
              school_id: "s",
              student_id: "s3",
              academic_year_id: "y1",
              route_id: "r1",
              stop_id: "st1",
              from_date: "2026-04-01",
              to_date: null,
              amount: null,
              notes: "",
              hlc: "",
              student_number: "2026-0003",
              student_name: "Omar Shah",
              class_name: "9",
              section_name: "A",
              section_id: "sec1",
              roll_no: 3,
              guardian_phone: "0300 1234567",
              route_name: "North",
              stop_name: "Gulshan",
              stop_position: 0,
              pickup_time: "07:10:00",
              charge: "1500.00",
            },
          ],
        },
        { stop: northDetail.stop_list[1]!, students: [] },
      ],
    });
    renderPage(["transport.read"]);
    await userEvent.click(screen.getByRole("tab", { name: "Roster" }));
    const roster = await screen.findByTestId("roster");
    expect(roster).toHaveTextContent("Omar Shah");
    expect(roster).toHaveTextContent("1 seat(s) left");
    expect(roster).toHaveTextContent("No students at this stop");
  });
});
