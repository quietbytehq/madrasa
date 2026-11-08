import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type { AdmissionsBoard, ApplicationDetail, ApplicationView, UserSummary } from "@/api/types";
import { AdmissionsPage } from "./AdmissionsPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    admissionsBoard: vi.fn(),
    application: vi.fn(),
    moveApplication: vi.fn(),
    moveApplications: vi.fn(),
    classes: vi.fn(),
    sections: vi.fn(),
    convertApplication: vi.fn(),
    createApplication: vi.fn(),
    updateApplication: vi.fn(),
    deleteApplication: vi.fn(),
    downloadOfferLetter: vi.fn(),
    admissionStages: vi.fn(),
    admissionSettings: vi.fn(),
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

const stages = [
  {
    id: "st-e",
    school_id: "s",
    code: "enquiry",
    name: "Enquiry",
    kind: "open" as const,
    position: 0,
    hlc: "",
  },
  {
    id: "st-o",
    school_id: "s",
    code: "offered",
    name: "Offered",
    kind: "open" as const,
    position: 1,
    hlc: "",
  },
  {
    id: "st-a",
    school_id: "s",
    code: "admitted",
    name: "Admitted",
    kind: "admitted" as const,
    position: 2,
    hlc: "",
  },
];

const app = (over: Partial<ApplicationView>): ApplicationView => ({
  id: "a1",
  school_id: "s",
  application_no: "APP-2026-0001",
  academic_year_id: "y",
  class_id: "c6",
  stage_id: "st-e",
  enquiry_date: "2026-02-10",
  source: "Walk-in",
  full_name: "Ali Raza",
  father_name: "Imran Raza",
  date_of_birth: "2015-03-14",
  gender: "male",
  cnic: null,
  address: "",
  phone: "0300 1234567",
  email: null,
  previous_school: "",
  guardian_name: "Imran Raza",
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
  custom: {},
  student_id: null,
  admitted_on: null,
  created_at: "2026-02-10T00:00:00Z",
  updated_at: "2026-02-10T00:00:00Z",
  hlc: "",
  stage_name: "Enquiry",
  stage_kind: "open",
  class_name: "6",
  student_number: null,
  ...over,
});

const board: AdmissionsBoard = {
  academic_year: { id: "y", name: "2026-27" } as AdmissionsBoard["academic_year"],
  stages,
  applications: [
    app({}),
    app({
      id: "a2",
      application_no: "APP-2026-0002",
      full_name: "Sara Noor",
      stage_id: "st-a",
      stage_name: "Admitted",
      stage_kind: "admitted",
    }),
  ],
  funnel: [
    { stage_id: "st-e", reached: 2, current: 1 },
    { stage_id: "st-o", reached: 1, current: 0 },
    { stage_id: "st-a", reached: 1, current: 1 },
  ],
};

const detail = (a: ApplicationView): ApplicationDetail => ({
  ...a,
  events: [
    {
      id: "e1",
      application_id: a.id,
      from_stage_id: null,
      to_stage_id: a.stage_id,
      to_stage_name: a.stage_name,
      note: "",
      changed_by: "u1",
      changed_by_name: "Ayesha Khan",
      changed_at: "2026-02-10T00:00:00Z",
    },
  ],
});

function renderPage(permissions: string[]) {
  return renderWithProviders(
    <FeaturesProvider initial={{ edition: "standard", features: ["dashboard", "admissions"] }}>
      <SessionProvider initial={{ user: admin, permissions }}>
        <AdmissionsPage />
      </SessionProvider>
    </FeaturesProvider>,
    { route: "/admissions" },
  );
}

describe("AdmissionsPage", () => {
  beforeEach(() => {
    vi.mocked(api.admissionsBoard).mockResolvedValue(board);
    vi.mocked(api.classes).mockResolvedValue([]);
    vi.mocked(api.sections).mockResolvedValue([
      { id: "sec1", name: "A", class_id: "c6", academic_year_id: "y" } as never,
    ]);
  });

  it("draws the pipeline with counts and moves a card from its stage selector", async () => {
    vi.mocked(api.moveApplication).mockResolvedValue(
      detail(app({ stage_id: "st-o", stage_name: "Offered" })),
    );
    renderPage(["admissions.read", "admissions.manage"]);
    const enquiry = await screen.findByTestId("stage-enquiry");
    expect(within(enquiry).getByText("Ali Raza")).toBeInTheDocument();
    expect(within(enquiry).getByText("2 reached")).toBeInTheDocument();
    expect(within(screen.getByTestId("stage-admitted")).getByText("Sara Noor")).toBeInTheDocument();
    await userEvent.selectOptions(
      within(enquiry).getByRole("combobox", { name: "Ali Raza: Move to" }),
      "st-o",
    );
    await waitFor(() => expect(api.moveApplication).toHaveBeenCalledWith("a1", "st-o"));
  });

  it("opens an admitted application and converts it into a student", async () => {
    const admitted = board.applications[1]!;
    vi.mocked(api.application).mockResolvedValue(detail(admitted));
    vi.mocked(api.convertApplication).mockResolvedValue({
      application: detail(app({ ...admitted, student_id: "stu1", student_number: "2026-0001" })),
      student_id: "stu1",
      student_number: "2026-0001",
      guardian_id: "g1",
      voucher_id: null,
    });
    renderPage(["admissions.read", "admissions.manage"]);
    await userEvent.click(await screen.findByTestId("card-APP-2026-0002"));
    expect(await screen.findByTestId("history")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("convert"));
    await userEvent.click(await screen.findByTestId("convert-confirm"));
    await waitFor(() =>
      expect(api.convertApplication).toHaveBeenCalledWith("a2", {
        section_id: "sec1",
        roll_no: null,
        admission_date: expect.any(String),
        student_number: null,
        force: false,
      }),
    );
    expect(await screen.findByText("Converted")).toBeInTheDocument();
  });

  it("gives a reader the board without the new or stages controls", async () => {
    renderPage(["admissions.read"]);
    await screen.findByTestId("stage-enquiry");
    expect(screen.queryByTestId("new-application")).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Stages" })).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Ali Raza: Move to" })).not.toBeInTheDocument();
  });
});
