import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { BuilderCatalogue, CustomReport, ReportDocument, UserSummary } from "@/api/types";
import { CustomReportBuilder } from "./CustomReportBuilder";
import { ReportsPage } from "./ReportsPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    reports: vi.fn(async () => []),
    customReports: vi.fn(),
    builderCatalogue: vi.fn(),
    previewCustomReport: vi.fn(),
    createCustomReport: vi.fn(),
    updateCustomReport: vi.fn(),
    deleteCustomReport: vi.fn(async () => undefined),
    runCustomReport: vi.fn(),
    downloadCustomReport: vi.fn(async () => undefined),
    academicYears: vi.fn(async () => []),
    classes: vi.fn(async () => []),
    sections: vi.fn(async () => []),
  },
}));

const catalogue: BuilderCatalogue = {
  sources: [
    {
      key: "students",
      label: "Students",
      fields: [
        {
          key: "full_name",
          label: "Name",
          type: "text",
          choices: [],
          operators: ["eq", "contains", "is_empty"],
        },
        {
          key: "gender",
          label: "Gender",
          type: "choice",
          choices: [
            { value: "male", label: "Male" },
            { value: "female", label: "Female" },
          ],
          operators: ["eq", "in"],
        },
        {
          key: "roll_no",
          label: "Roll no.",
          type: "number",
          choices: [],
          operators: ["eq", "between"],
        },
        { key: "section", label: "Section", type: "text", choices: [], operators: ["eq"] },
      ],
    },
    {
      key: "staff",
      label: "Staff",
      fields: [{ key: "full_name", label: "Name", type: "text", choices: [], operators: ["eq"] }],
    },
  ],
  operators: [
    { value: "eq", label: "is" },
    { value: "contains", label: "contains" },
    { value: "is_empty", label: "is empty" },
    { value: "in", label: "is one of" },
    { value: "between", label: "between" },
  ],
  default_limit: 500,
  max_limit: 5000,
};

const doc: ReportDocument = {
  code: "custom",
  lang: "en",
  school_name: "Gulshan High",
  title: "Boys of class 1",
  subtitle: null,
  params: [],
  columns: [
    { label: "Name", align: "start", stretch: true },
    { label: "Roll no.", align: "end", stretch: false },
  ],
  groups: [{ heading: "Section: A", rows: [["Ali Khan", "1"]], subtotal: ["Subtotal (1)", "1"] }],
  total: ["Total (1)", "1"],
  notes: [],
  landscape: false,
  generated_on: "06-11-2026",
  row_count: 1,
};

const saved: CustomReport = {
  id: "r1",
  name: "Boys of class 1",
  description: "Sports day",
  source: "students",
  definition: {
    source: "students",
    columns: ["full_name", "roll_no"],
    filters: [{ field: "gender", op: "eq", value: "male", value2: null }],
    sort: [],
    group_by: "section",
    limit: null,
  },
  owner_user_id: "u1",
  owner_name: "Ayesha",
  is_shared: true,
  can_edit: true,
  updated_at: "2026-11-06T00:00:00Z",
};

const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "ayesha",
  full_name: "Ayesha",
  email: null,
  is_active: true,
  roles: ["admin"],
};

function renderAt(route: string, permissions: string[]) {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions }}>
      <Routes>
        <Route path="/reports/custom/new" element={<CustomReportBuilder />} />
        <Route path="/reports/custom/:id/edit" element={<CustomReportBuilder />} />
        <Route path="/reports/custom/:id" element={<ReportsPage />} />
        <Route path="/reports/:code?" element={<ReportsPage />} />
      </Routes>
    </SessionProvider>,
    { route },
  );
}

beforeEach(() => {
  vi.mocked(api.builderCatalogue).mockResolvedValue(catalogue);
  vi.mocked(api.customReports).mockReset();
  vi.mocked(api.customReports).mockResolvedValue([saved]);
  vi.mocked(api.previewCustomReport).mockReset();
  vi.mocked(api.createCustomReport).mockReset();
  vi.mocked(api.updateCustomReport).mockReset();
  vi.mocked(api.runCustomReport).mockResolvedValue(doc);
});

describe("CustomReportBuilder", () => {
  it("builds a definition from the catalogue, previews it and saves it shared", async () => {
    vi.mocked(api.previewCustomReport).mockResolvedValue(doc);
    vi.mocked(api.createCustomReport).mockResolvedValue(saved);
    renderAt("/reports/custom/new", ["reports.read", "reports.manage"]);
    expect(await screen.findByRole("heading", { name: "New custom report" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save report" })).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Report name"), "Boys of class 1");
    const columns = screen.getByRole("list", { name: "Columns" });
    await userEvent.click(within(columns).getByLabelText("Roll no."));
    await userEvent.click(within(columns).getByLabelText("Name"));
    expect(within(columns).getByText("2")).toBeInTheDocument();
    await userEvent.click(within(columns).getByRole("button", { name: "Move up: Name" }));
    await userEvent.click(screen.getByRole("button", { name: "Add filter" }));
    await userEvent.selectOptions(screen.getAllByLabelText("Filters")[0]!, "gender");
    await userEvent.selectOptions(screen.getByLabelText("Value"), "male");
    await userEvent.selectOptions(screen.getByLabelText("Group by"), "section");
    await userEvent.click(screen.getByLabelText("Share with everyone who can run reports"));
    await userEvent.click(screen.getByRole("button", { name: "Preview" }));
    await waitFor(() =>
      expect(api.previewCustomReport).toHaveBeenCalledWith(
        {
          source: "students",
          columns: ["full_name", "roll_no"],
          filters: [{ field: "gender", op: "eq", value: "male", value2: null }],
          sort: [],
          group_by: "section",
          limit: null,
        },
        "Boys of class 1",
        "en",
      ),
    );
    expect(await screen.findByText("Section: A")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Save report" }));
    await waitFor(() =>
      expect(api.createCustomReport).toHaveBeenCalledWith(
        expect.objectContaining({ name: "Boys of class 1", is_shared: true }),
      ),
    );
    expect(await screen.findByText("Report saved.")).toBeInTheDocument();
    expect(await screen.findByText("Sports day")).toBeInTheDocument();
  });

  it("edits a saved report, cannot share without reports.manage, and switching the source resets it", async () => {
    vi.mocked(api.updateCustomReport).mockResolvedValue(saved);
    renderAt("/reports/custom/r1/edit", ["reports.read"]);
    expect(await screen.findByRole("heading", { name: "Edit report" })).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByLabelText("Report name")).toHaveValue("Boys of class 1"),
    );
    expect(
      screen.queryByLabelText("Share with everyone who can run reports"),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/Only the principal and administrators can share/)).toBeInTheDocument();
    const columns = screen.getByRole("list", { name: "Columns" });
    expect(within(columns).getByLabelText("Name")).toBeChecked();
    expect(screen.getByLabelText("Value")).toHaveValue("male");
    await userEvent.selectOptions(screen.getByLabelText("Data"), "staff");
    expect(screen.getByText("Tick at least one column.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save report" })).toBeDisabled();
  });
});

describe("Reports library with custom reports", () => {
  it("lists saved reports with edit and remove, runs one and downloads it", async () => {
    renderAt("/reports/custom/r1", ["reports.read", "reports.manage"]);
    const nav = await screen.findByRole("navigation", { name: "Report library" });
    expect(within(nav).getByText("Custom reports")).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /^Boys of class 1/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByRole("link", { name: "Edit: Boys of class 1" })).toHaveAttribute(
      "href",
      "/reports/custom/r1/edit",
    );
    await waitFor(() => expect(api.runCustomReport).toHaveBeenCalledWith("r1", "en"));
    expect(await screen.findByText("Ali Khan")).toBeInTheDocument();
    expect(screen.getByText("Shared · by Ayesha")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Download Excel" }));
    await waitFor(() => expect(api.downloadCustomReport).toHaveBeenCalledWith("r1", "en", "xlsx"));
    vi.spyOn(window, "confirm").mockReturnValue(true);
    await userEvent.click(within(nav).getByRole("button", { name: "Remove: Boys of class 1" }));
    await waitFor(() => expect(api.deleteCustomReport).toHaveBeenCalledWith("r1"));
    expect(await screen.findByText("Report removed.")).toBeInTheDocument();
  });

  it("hides custom reports from a teacher who only has exam reports", async () => {
    renderAt("/reports", ["exams.read"]);
    const nav = await screen.findByRole("navigation", { name: "Report library" });
    expect(within(nav).queryByText("Custom reports")).not.toBeInTheDocument();
    expect(api.customReports).not.toHaveBeenCalled();
  });
});
