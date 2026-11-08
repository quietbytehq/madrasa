import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type {
  AllocationView,
  HostelBoard,
  HostelDetail,
  HostelView,
  HostelRegisterEntry,
  UserSummary,
} from "@/api/types";
import { HostelPage } from "./HostelPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    hostels: vi.fn(),
    hostel: vi.fn(),
    createHostel: vi.fn(),
    updateHostel: vi.fn(),
    deleteHostel: vi.fn(),
    createHostelRoom: vi.fn(),
    updateHostelRoom: vi.fn(),
    deleteHostelRoom: vi.fn(),
    hostelBoard: vi.fn(),
    hostelResidents: vi.fn(),
    allocateBed: vi.fn(),
    updateAllocation: vi.fn(),
    hostelRegister: vi.fn(),
    openRegister: vi.fn(),
    closeRegister: vi.fn(),
    academicYears: vi.fn(),
    students: vi.fn(),
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

const iqbal: HostelView = {
  id: "h1",
  school_id: "s",
  name: "Iqbal House",
  kind: "boys",
  warden_name: "Mr Rashid",
  warden_phone: "",
  monthly_fee: "8000.00",
  fee_head_id: "fh1",
  requires_consent: true,
  active: true,
  notes: "",
  custom: {},
  hlc: "",
  fee_head_name: "Hostel",
  rooms: 1,
  beds: 2,
  occupied: 1,
};

const room = {
  id: "r1",
  school_id: "s",
  hostel_id: "h1",
  floor: "Ground",
  name: "G-1",
  beds: 2,
  monthly_fee: null,
  active: true,
  notes: "",
  hlc: "",
  occupied: 1,
};

const ali: AllocationView = {
  id: "a1",
  school_id: "s",
  student_id: "s1",
  academic_year_id: "y1",
  hostel_id: "h1",
  room_id: "r1",
  bed_no: 1,
  from_date: "2026-04-01",
  to_date: null,
  amount: null,
  notes: "",
  hlc: "",
  student_number: "2026-0001",
  student_name: "Ali Khan",
  gender: "male",
  class_name: "9",
  section_name: "A",
  roll_no: 1,
  guardian_phone: "0300 1234567",
  hostel_name: "Iqbal House",
  room_name: "G-1",
  floor: "Ground",
  charge: "8000.00",
};

const detail: HostelDetail = { ...iqbal, room_list: [room] };

const board: HostelBoard = {
  ...iqbal,
  academic_year_id: "y1",
  by_room: [
    {
      room,
      beds: [
        { bed_no: 1, resident: ali },
        { bed_no: 2, resident: null },
      ],
    },
  ],
  occupancy: { beds: 2, occupied: 1, free: 1 },
};

function renderPage(permissions: string[]) {
  return renderWithProviders(
    <FeaturesProvider
      initial={{ edition: "standard", features: ["dashboard", "students", "hostel"] }}
    >
      <SessionProvider initial={{ user: admin, permissions }}>
        <HostelPage />
      </SessionProvider>
    </FeaturesProvider>,
    { route: "/hostel" },
  );
}

describe("HostelPage", () => {
  beforeEach(() => {
    vi.mocked(api.hostels).mockResolvedValue([iqbal]);
    vi.mocked(api.hostel).mockResolvedValue(detail);
    vi.mocked(api.hostelBoard).mockResolvedValue(board);
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
  });

  it("shows the board and allocates a free bed to a searched student", async () => {
    vi.mocked(api.students).mockResolvedValue({
      items: [
        {
          id: "s2",
          student_number: "2026-0002",
          full_name: "Sara Noor",
          father_name: "F",
          gender: "female",
          date_of_birth: "2015-01-01",
          status: "active",
          admission_date: "2026-04-01",
          class_id: "c9",
          class_name: "9",
          section_id: "sec1",
          section_name: "A",
          roll_no: 2,
          has_photo: false,
        },
      ],
      page: 1,
      page_size: 8,
      total: 1,
    });
    vi.mocked(api.allocateBed).mockResolvedValue({ ...ali, id: "a2", student_id: "s2", bed_no: 2 });
    renderPage(["hostel.read", "hostel.manage"]);
    const b = await screen.findByTestId("board");
    expect(b).toHaveTextContent("Ali Khan");
    expect(b).toHaveTextContent("1 / 2 beds taken");
    await userEvent.click(within(b).getByRole("button", { name: "G-1 2" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Student"), "Sa");
    await userEvent.click(await within(dialog).findByText("Sara Noor"));
    await userEvent.click(within(dialog).getByTestId("allocate"));
    await waitFor(() =>
      expect(api.allocateBed).toHaveBeenCalledWith(
        expect.objectContaining({
          student_id: "s2",
          room_id: "r1",
          bed_no: 2,
          academic_year_id: "y1",
        }),
      ),
    );
  });

  it("hides the allocate controls from readers", async () => {
    renderPage(["hostel.read"]);
    const b = await screen.findByTestId("board");
    expect(within(b).queryByRole("button", { name: "G-1 2" })).not.toBeInTheDocument();
    expect(b).toHaveTextContent("Free");
  });

  it("records a leave-out with consent and a return", async () => {
    const out: HostelRegisterEntry = {
      id: "e1",
      school_id: "s",
      hostel_id: "h1",
      student_id: "s1",
      kind: "leave_out",
      started_at: "2026-06-05T09:00:00Z",
      expected_back: "2020-01-01T18:00:00Z",
      ended_at: null,
      visitor_name: "",
      visitor_relationship: "",
      visitor_phone: "",
      guardian_consent: true,
      consent_note: "Father phoned",
      recorded_by: "u1",
      closed_by: null,
      notes: "",
      hlc: "",
      student_number: "2026-0001",
      student_name: "Ali Khan",
      hostel_name: "Iqbal House",
      room_name: "G-1",
    };
    vi.mocked(api.hostelRegister).mockResolvedValue([out]);
    vi.mocked(api.hostelResidents).mockResolvedValue([ali]);
    vi.mocked(api.openRegister).mockResolvedValue(out);
    vi.mocked(api.closeRegister).mockResolvedValue({ ...out, ended_at: "2026-06-07T17:00:00Z" });
    renderPage(["hostel.read", "hostel.manage"]);
    await userEvent.click(screen.getByRole("tab", { name: "Register" }));
    expect(await screen.findByText("Ali Khan")).toBeInTheDocument();
    expect(screen.getByText(/Overdue/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Returned" }));
    await waitFor(() => expect(api.closeRegister).toHaveBeenCalledWith("e1"));
    await userEvent.click(screen.getByTestId("add-entry"));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Resident"), "Al");
    await userEvent.click(await within(dialog).findByText("Ali Khan"));
    await userEvent.click(within(dialog).getByLabelText("Guardian consent recorded"));
    await userEvent.click(within(dialog).getByTestId("save-entry"));
    await waitFor(() =>
      expect(api.openRegister).toHaveBeenCalledWith(
        expect.objectContaining({ student_id: "s1", kind: "leave_out", guardian_consent: true }),
      ),
    );
  });
});
