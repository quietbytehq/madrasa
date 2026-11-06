import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { ReportCatalogueModule, ReportDocument, UserSummary } from "@/api/types";
import { ReportsPage } from "./ReportsPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    reports: vi.fn(),
    customReports: vi.fn(async () => []),
    runReport: vi.fn(),
    downloadReport: vi.fn(async () => undefined),
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
    classes: vi.fn(async () => [{ id: "c9", school_id: "s", name: "9", level: 9 }]),
    assessments: vi.fn(async () => [
      {
        id: "a1",
        school_id: "s",
        academic_year_id: "y1",
        name: "Mid-year",
        position: 0,
        weight: "30.00",
      },
    ]),
    sections: vi.fn(async () => [
      {
        id: "sec9a",
        school_id: "s",
        class_id: "c9",
        academic_year_id: "y1",
        name: "A",
        class_teacher_id: null,
        capacity: null,
      },
    ]),
  },
}));

const catalogue: ReportCatalogueModule[] = [
  {
    module: "students",
    label: "Students",
    reports: [
      {
        code: "students.list",
        title: "Student list",
        description: "Students with class, section and guardian phone.",
        landscape: true,
        access: "reports_read",
        params: [
          {
            name: "academic_year_id",
            kind: "academic_year",
            label: "Academic year",
            required: false,
          },
          { name: "class_id", kind: "class", label: "Class", required: false },
          { name: "section_id", kind: "section", label: "Section", required: false },
          { name: "status", kind: "status", label: "Status", required: false },
        ],
      },
      {
        code: "students.birthdays",
        title: "Birthdays",
        description: "Born in a month.",
        landscape: false,
        access: "reports_read",
        params: [
          {
            name: "academic_year_id",
            kind: "academic_year",
            label: "Academic year",
            required: false,
          },
          { name: "month", kind: "month", label: "Month", required: true },
        ],
      },
    ],
  },
];

const document: ReportDocument = {
  code: "students.list",
  lang: "en",
  school_name: "Gulshan High",
  title: "Student list",
  subtitle: null,
  params: [
    { label: "Academic year", value: "2026-27" },
    { label: "Class", value: "9" },
  ],
  columns: [
    { label: "No.", align: "end", stretch: false },
    { label: "Name", align: "start", stretch: true },
    { label: "Phone", align: "start", stretch: false },
  ],
  groups: [
    {
      heading: "Class 9 · A",
      rows: [
        ["1", "Ali Khan", "0300-1111111"],
        ["2", "Sara Khan", "0300-2222222"],
      ],
      subtotal: ["", "Subtotal", "2"],
    },
  ],
  total: ["", "Total", "2"],
  notes: [],
  landscape: true,
  generated_on: "06-11-2026",
  row_count: 2,
};

beforeEach(() => {
  vi.mocked(api.reports).mockResolvedValue(catalogue);
  vi.mocked(api.runReport).mockReset();
  vi.mocked(api.downloadReport).mockClear();
});

describe("ReportsPage", () => {
  it("lists the library by module and opens a report", async () => {
    renderAt("/reports");
    const nav = await screen.findByRole("navigation", { name: "Report library" });
    expect(within(nav).getByText("Students")).toBeInTheDocument();
    expect(screen.getByText("Choose a report from the list.")).toBeInTheDocument();
    await userEvent.click(within(nav).getByRole("link", { name: /Student list/ }));
    expect(
      await screen.findByText("Students with class, section and guardian phone."),
    ).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /Student list/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("runs a report with its parameters, shows groups and totals, and downloads PDF and Excel", async () => {
    vi.mocked(api.runReport).mockResolvedValue(document);
    renderAt("/reports/students.list");
    await screen.findByText("Students with class, section and guardian phone.");
    await waitFor(() => expect(screen.getByLabelText("Academic year")).toHaveValue("y1"));
    expect(screen.getByLabelText("Section")).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText("Class"), "c9");
    await waitFor(() => expect(screen.getByLabelText("Section")).toBeEnabled());
    await userEvent.selectOptions(screen.getByLabelText("Section"), "sec9a");
    await userEvent.selectOptions(screen.getByLabelText("Status"), "left");
    await userEvent.click(screen.getByRole("button", { name: "Run report" }));
    await waitFor(() =>
      expect(api.runReport).toHaveBeenCalledWith(
        "students.list",
        { academic_year_id: "y1", class_id: "c9", section_id: "sec9a", status: "left" },
        "en",
      ),
    );
    expect(await screen.findByText("Class 9 · A")).toBeInTheDocument();
    expect(screen.getByText("Ali Khan")).toBeInTheDocument();
    expect(screen.getByText("Subtotal")).toBeInTheDocument();
    expect(screen.getByText("Total")).toBeInTheDocument();
    expect(screen.getByText("2 rows")).toBeInTheDocument();
    expect(screen.getByText("Gulshan High")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Download PDF" }));
    await waitFor(() =>
      expect(api.downloadReport).toHaveBeenCalledWith(
        "students.list",
        expect.objectContaining({ class_id: "c9" }),
        "en",
        "pdf",
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Download Excel" }));
    await waitFor(() =>
      expect(api.downloadReport).toHaveBeenLastCalledWith(
        "students.list",
        expect.anything(),
        "en",
        "xlsx",
      ),
    );
  });

  it("requires a month for the birthdays report and defaults it to the current month", async () => {
    renderAt("/reports/students.birthdays");
    const month = await screen.findByLabelText("Month");
    await waitFor(() => expect((month as HTMLInputElement).value).toMatch(/^\d{4}-\d{2}$/));
    await userEvent.clear(month);
    expect(screen.getByRole("button", { name: "Run report" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Download PDF" })).toBeDisabled();
  });

  it("offers assessment, number and student-number controls and runs from a shortcut link", async () => {
    vi.mocked(api.reports).mockResolvedValue([
      {
        module: "exams",
        label: "Examinations",
        reports: [
          {
            code: "exams.merit",
            title: "Merit list",
            description: "Top students.",
            landscape: false,
            access: "exams_read",
            params: [
              {
                name: "academic_year_id",
                kind: "academic_year",
                label: "Academic year",
                required: false,
              },
              { name: "assessment_id", kind: "assessment", label: "Assessment", required: true },
              { name: "class_id", kind: "class", label: "Class", required: true },
              { name: "top", kind: "number", label: "Top", required: false },
            ],
          },
        ],
      },
      {
        module: "fees",
        label: "Fees & Finance",
        reports: [
          {
            code: "fees.ledger",
            title: "Student fee ledger",
            description: "One student.",
            landscape: true,
            access: "reports_read",
            params: [
              {
                name: "student_number",
                kind: "student_number",
                label: "Student number",
                required: true,
              },
            ],
          },
          {
            code: "fees.collection",
            title: "Collection (receipts)",
            description: "Receipts.",
            landscape: true,
            access: "reports_read",
            params: [
              { name: "from", kind: "date_from", label: "From", required: true },
              { name: "to", kind: "date_to", label: "To", required: true },
            ],
          },
        ],
      },
    ]);
    vi.mocked(api.runReport).mockResolvedValue({
      ...document,
      code: "fees.collection",
      title: "Collection",
    });
    renderAt("/reports/exams.merit");
    await screen.findByText("Top students.");
    await waitFor(() => expect(api.assessments).toHaveBeenCalledWith("y1"));
    expect(screen.getByRole("button", { name: "Run report" })).toBeDisabled();
    await userEvent.selectOptions(await screen.findByLabelText("Assessment"), "a1");
    await userEvent.selectOptions(screen.getByLabelText("Class"), "c9");
    expect(screen.getByLabelText("Top")).toHaveValue(10);
    expect(screen.getByRole("button", { name: "Run report" })).toBeEnabled();

    renderAt("/reports/fees.ledger");
    const number = await screen.findByLabelText("Student number");
    await userEvent.type(number, "KHI-MC-2026-00001");
    expect(number).toHaveValue("KHI-MC-2026-00001");

    // A dashboard shortcut carries the dates and `run`: the report runs on its own.
    renderAt("/reports/fees.collection?from=2026-11-06&to=2026-11-06&run");
    await waitFor(() =>
      expect(api.runReport).toHaveBeenCalledWith(
        "fees.collection",
        { from: "2026-11-06", to: "2026-11-06" },
        "en",
      ),
    );
    expect((await screen.findAllByText("Collection")).length).toBeGreaterThan(0);
  });

  it("asks for the catalogue and the report in Urdu and renders right-to-left", async () => {
    vi.mocked(api.reports).mockResolvedValue([
      {
        module: "students",
        label: "طلبہ",
        reports: [{ ...catalogue[0]!.reports[0]!, title: "طلبہ کی فہرست" }],
      },
    ]);
    vi.mocked(api.runReport).mockResolvedValue({ ...document, lang: "ur", title: "طلبہ کی فہرست" });
    renderAt("/reports/students.list", "ur");
    await waitFor(() => expect(api.reports).toHaveBeenCalledWith("ur"));
    await userEvent.click(await screen.findByRole("button", { name: "رپورٹ چلائیں" }));
    await waitFor(() =>
      expect(api.runReport).toHaveBeenCalledWith("students.list", expect.anything(), "ur"),
    );
    const table = await screen.findByRole("table");
    expect(table.closest("[dir]")).toHaveAttribute("dir", "rtl");
  });
});

const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "ayesha",
  full_name: "Ayesha",
  email: null,
  is_active: true,
  roles: ["admin"],
};

function renderAt(route: string, locale: "en" | "ur" = "en") {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions: ["reports.read"] }}>
      <Routes>
        <Route path="/reports/:code?" element={<ReportsPage />} />
      </Routes>
    </SessionProvider>,
    { route, locale },
  );
}
