import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { ConsentType, SectionConsents, StudentConsents, UserSummary } from "@/api/types";
import { ConsentsPage } from "./ConsentsPage";
import { StudentConsentsTab } from "./StudentConsentsTab";
import { ConsentTypesCard } from "../academics/ConsentTypesCard";

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
    classes: vi.fn(async () => [{ id: "c1", school_id: "s", name: "1", level: 1 }]),
    sections: vi.fn(async () => [
      {
        id: "sa",
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
    consentTypes: vi.fn(),
    createConsentType: vi.fn(),
    updateConsentType: vi.fn(),
    deleteConsentType: vi.fn(),
    studentConsents: vi.fn(),
    recordConsent: vi.fn(),
    sectionConsents: vi.fn(),
    recordSectionConsents: vi.fn(),
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

const photos: ConsentType = {
  id: "t1",
  school_id: "s",
  name: "Photos",
  description: "Use of photos in school publications",
  given_by: "guardian",
  required: "all",
  min_level: null,
  max_level: null,
  renewal: "none",
  position: 0,
  hlc: "",
};
const trips: ConsentType = {
  ...photos,
  id: "t2",
  name: "Trips",
  description: "",
  renewal: "yearly",
  position: 1,
};
const guardians = [{ id: "g1", full_name: "Ahmed Khan", relationship: "father" }];

describe("ConsentsPage", () => {
  it("shows the section matrix and records chosen states in bulk", async () => {
    const view: SectionConsents = {
      section_id: "sa",
      academic_year_id: "y",
      class_name: "1",
      section_name: "A",
      class_level: 1,
      types: [photos, trips],
      students: [
        {
          student_id: "ali",
          student_number: "KHI-MC-2026-00001",
          full_name: "Ali Khan",
          roll_no: 1,
          guardians,
          cells: [
            {
              consent_type_id: "t1",
              required: true,
              current: "given",
              recorded_on: "2026-04-02",
              guardian_name: "Ahmed Khan",
            },
            {
              consent_type_id: "t2",
              required: true,
              current: "expired",
              recorded_on: "2025-05-01",
              guardian_name: "Ahmed Khan",
            },
          ],
        },
      ],
      missing: 1,
    };
    vi.mocked(api.sectionConsents).mockResolvedValue(view);
    vi.mocked(api.recordSectionConsents).mockResolvedValue({ ...view, missing: 0 });
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <ConsentsPage />
      </SessionProvider>,
      { route: "/students/consents" },
    );
    await screen.findByRole("option", { name: "1 A" });
    await userEvent.selectOptions(screen.getByLabelText("Section"), "sa");
    const table = await screen.findByRole("table", { name: "Consents" });
    expect(screen.getByText("1 missing")).toBeInTheDocument();
    const trip = within(table).getByLabelText("Ali Khan: Trips");
    expect(within(trip).getByRole("option", { name: "Expired" })).toBeInTheDocument();
    await userEvent.selectOptions(trip, "given");
    await userEvent.click(screen.getByRole("button", { name: "Save consents" }));
    await waitFor(() =>
      expect(api.recordSectionConsents).toHaveBeenCalledWith(
        "sa",
        [{ student_id: "ali", consent_type_id: "t2", status: "given", guardian_id: "g1" }],
        expect.any(String),
      ),
    );
  });
});

describe("StudentConsentsTab", () => {
  it("lists states and history and records a refusal", async () => {
    const mine: StudentConsents = {
      academic_year_id: "y",
      class_level: 1,
      types: [
        { ...photos, needed: true, current: "missing", latest: null },
        { ...trips, needed: true, current: "given", latest: null },
      ],
      guardians,
      history: [
        {
          id: "h1",
          school_id: "s",
          student_id: "ali",
          consent_type_id: "t2",
          status: "given",
          guardian_id: "g1",
          academic_year_id: "y",
          recorded_on: "2026-04-02",
          recorded_by: "u1",
          note: "form 12",
          created_at: "2026-04-02T10:00:00Z",
          type_name: "Trips",
          guardian_name: "Ahmed Khan",
          recorded_by_name: "Ayesha Khan",
        },
      ],
    };
    vi.mocked(api.studentConsents).mockResolvedValue(mine);
    vi.mocked(api.recordConsent).mockResolvedValue(mine);
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <StudentConsentsTab studentId="ali" manage />
      </SessionProvider>,
    );
    const table = await screen.findByRole("table", { name: "Consents" });
    expect(within(table).getByText("Missing")).toBeInTheDocument();
    expect(within(table).getByText("Given")).toBeInTheDocument();
    const history = screen.getByRole("list", { name: "History" });
    expect(within(history).getByText("recorded by Ayesha Khan")).toBeInTheDocument();
    // A guardian consent marked given needs the guardian; a refusal does not.
    const button = screen.getByRole("button", { name: "Record" });
    expect(button).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText("Status"), "refused");
    expect(button).toBeEnabled();
    await userEvent.type(screen.getByLabelText("Note"), "declined on the form");
    await userEvent.click(button);
    await waitFor(() =>
      expect(api.recordConsent).toHaveBeenCalledWith("ali", {
        consent_type_id: "t1",
        status: "refused",
        guardian_id: null,
        recorded_on: expect.any(String),
        note: "declined on the form",
        academic_year_id: "y",
      }),
    );
  });
});

describe("ConsentTypesCard", () => {
  it("lists types and creates one with a class range", async () => {
    vi.mocked(api.consentTypes).mockResolvedValue([photos, trips]);
    vi.mocked(api.createConsentType).mockResolvedValue(photos);
    renderWithProviders(
      <SessionProvider initial={{ user: admin, permissions: ADMIN }}>
        <ConsentTypesCard manage onError={() => undefined} />
      </SessionProvider>,
    );
    const table = await screen.findByRole("table", { name: "Consent types" });
    expect(within(table).getByText("Photos")).toBeInTheDocument();
    expect(within(table).getAllByText("Every year")).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "New consent type" }));
    const editor = screen.getByTestId("consent-type-editor");
    await userEvent.type(within(editor).getByLabelText("Name"), "Lab safety");
    await userEvent.selectOptions(within(editor).getByLabelText("Required for"), "class_range");
    await userEvent.type(within(editor).getByLabelText("From level"), "9");
    await userEvent.type(within(editor).getByLabelText("To level"), "12");
    await userEvent.selectOptions(within(editor).getByLabelText("Given by"), "either");
    await userEvent.click(within(editor).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(api.createConsentType).toHaveBeenCalledWith({
        name: "Lab safety",
        description: "",
        given_by: "either",
        required: "class_range",
        min_level: 9,
        max_level: 12,
        renewal: "none",
        position: 2,
      }),
    );
  });
});
