import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { ImportField, ImportPreview, ParsedFile, UserSummary } from "@/api/types";
import { errorCsv, ImportPage } from "./ImportPage";
import { importDraft } from "./importDraft";

vi.mock("@/api/endpoints", () => ({
  api: {
    importFields: vi.fn(),
    importBatches: vi.fn(async () => []),
    parseImport: vi.fn(),
    previewImport: vi.fn(),
    applyImport: vi.fn(),
    downloadImportReport: vi.fn().mockResolvedValue(undefined),
    rollbackImport: vi.fn(),
    downloadTemplate: vi.fn(async () => {}),
  },
}));

const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "ayesha",
  full_name: "Ayesha Khan",
  email: null,
  is_active: true,
  roles: ["admin"],
};
const fields: ImportField[] = [
  { key: "full_name", label: "Student name", required: true, aliases: [], example: "Ali" },
  { key: "father_name", label: "Father's name", required: true, aliases: [], example: "Ahmed" },
  {
    key: "date_of_birth",
    label: "Date of birth",
    required: true,
    aliases: [],
    example: "2015-01-01",
  },
  { key: "gender", label: "Gender", required: true, aliases: [], example: "male" },
  { key: "class", label: "Class", required: true, aliases: [], example: "1" },
  { key: "guardian_phone", label: "Guardian phone", required: false, aliases: [], example: "" },
  { key: "first_name", label: "First name", required: false, aliases: [], example: "Ali" },
  { key: "last_name", label: "Last name", required: false, aliases: [], example: "Khan" },
];
const parsed: ParsedFile = {
  file_name: "students.xlsx",
  sheets: ["Sheet1"],
  sheet: "Sheet1",
  headers: ["Student Name", "Father", "DOB", "Sex", "Class", "Mobile"],
  rows: [
    ["Ali Khan", "Ahmed", "04/03/2015", "M", "1-A", "0300 1234567"],
    ["Sara Khan", "Ahmed", "not a date", "F", "1-A", ""],
  ],
  mapping: { full_name: 0, father_name: 1, gender: 3, class: 4, guardian_phone: 5 },
  mapping_saved: false,
};
const preview: ImportPreview = {
  rows: [
    {
      row_no: 2,
      values: { full_name: "Ali Khan" },
      errors: [],
      warnings: [],
      action: "create",
      matched: null,
    },
    {
      row_no: 3,
      values: { full_name: "Sara Khan" },
      errors: [
        {
          field: "date_of_birth",
          message: "Date of birth is not a date (use YYYY-MM-DD or DD/MM/YYYY)",
        },
      ],
      warnings: [],
      action: "error",
      matched: null,
    },
  ],
  summary: { total: 2, create: 1, update: 0, skip: 0, error: 1 },
  unmapped: [],
  report: {
    reasons: [
      { action: "error", reason: "Date of birth is not a date", count: 1, rows: [3] },
      { action: "error", reason: 'Class "7" is not set up in Settings', count: 1, rows: [3] },
    ],
    missing_classes: [{ name: "7", rows: [3] }],
    missing_sections: [{ name: "2-C", rows: [3] }],
    likely_duplicates: [
      { row_no: 2, name: "Ali Raza", looks_like: "Ali Raza (KHI-1), same father's name" },
    ],
  },
};

function render() {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions: ["students.manage", "teachers.manage"] }}>
      <ImportPage />
    </SessionProvider>,
  );
}

beforeEach(() => {
  vi.mocked(api.importFields).mockResolvedValue(fields);
  vi.mocked(api.parseImport).mockResolvedValue(parsed);
  vi.mocked(api.previewImport).mockClear();
  vi.mocked(api.previewImport).mockResolvedValue(preview);
  vi.mocked(api.applyImport).mockClear();
  importDraft.clear();
});

describe("ImportPage", () => {
  it("offers each kind of import to whoever may do it", async () => {
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ["fees.manage"] }}>
        <ImportPage />
      </SessionProvider>,
    );
    // An accountant imports the fee structure and nothing else.
    expect(await screen.findByRole("button", { name: /^Fee structure/ })).toBeEnabled();
    for (const name of [/^Students and guardians/, /^Guardians/, /^Staff/, /^Opening fee/]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    expect(screen.queryByRole("button", { name: "Data health" })).not.toBeInTheDocument();
  });

  it("asks which sheet to read when a workbook has several, preselecting Students over Read Me", async () => {
    const multi: ParsedFile = {
      ...parsed,
      sheets: ["Read Me", "Students", "Staff"],
      sheet: "Students",
    };
    vi.mocked(api.parseImport).mockClear();
    vi.mocked(api.parseImport).mockResolvedValue(multi);
    render();
    await userEvent.click(await screen.findByRole("button", { name: /Students and guardians/ }));
    const file = new File(["x"], "MapleRidge.xlsx", {
      type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    });
    await userEvent.upload(screen.getByLabelText("Choose a .xlsx or .csv file"), file);
    // The picker appears before the mapping step, with the data sheet already chosen.
    expect(await screen.findByText("Which sheet holds the data?")).toBeInTheDocument();
    const picker = screen.getByLabelText("Sheet");
    expect(picker).toHaveValue("Students");
    expect(api.parseImport).toHaveBeenCalledTimes(1);
    // Keeping the choice needs no second parse; choosing another sheet parses again with it.
    await userEvent.selectOptions(picker, "Staff");
    vi.mocked(api.parseImport).mockResolvedValue({ ...multi, sheet: "Staff" });
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(api.parseImport).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.parseImport).mock.calls[1]?.[0]).toMatchObject({
      file_name: "MapleRidge.xlsx",
      sheet: "Staff",
    });
    expect(await screen.findByText("2 rows found in students.xlsx")).toBeInTheDocument();
  });

  it("walks through type, file, columns, check and import", async () => {
    vi.mocked(api.applyImport).mockResolvedValue({
      batch: {
        id: "b1",
        school_id: "s",
        kind: "students",
        file_name: "students.xlsx",
        status: "applied",
        options: {},
        total_rows: 2,
        created_rows: 1,
        updated_rows: 0,
        skipped_rows: 1,
        applied_by: "u1",
        applied_at: "2026-10-30T10:00:00Z",
        rolled_back_at: null,
      },
      summary: preview.summary,
      rows: preview.rows,
    });
    render();
    await userEvent.click(await screen.findByRole("button", { name: /Students and guardians/ }));
    expect(screen.getByRole("button", { name: "Excel template" })).toBeInTheDocument();
    const file = new File(["x"], "students.xlsx", {
      type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    });
    await userEvent.upload(screen.getByLabelText("Choose a .xlsx or .csv file"), file);
    await waitFor(() =>
      expect(api.parseImport).toHaveBeenCalledWith(
        expect.objectContaining({ kind: "students", file_name: "students.xlsx" }),
      ),
    );
    // Columns step: the date of birth is required and unmapped, so checking is disabled.
    expect(await screen.findByText("2 rows found in students.xlsx")).toBeInTheDocument();
    const check = screen.getByRole("button", { name: "Check the file" });
    expect(check).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText("Date of birth"), "2");
    expect(check).toBeEnabled();
    await userEvent.selectOptions(screen.getByLabelText("If a record already exists"), "update");
    await userEvent.click(check);
    await waitFor(() =>
      expect(api.previewImport).toHaveBeenCalledWith(
        expect.objectContaining({
          mapping: expect.objectContaining({ date_of_birth: 2, full_name: 0 }),
          options: { duplicates: "update" },
          save_mapping: true,
        }),
      ),
    );
    // Check step: summary, error row editable, fixed value goes back to the server.
    expect(
      await screen.findByText("1 to add, 0 to update, 0 to skip, 1 with errors"),
    ).toBeInTheDocument();
    // The dry run in words: nothing saved, the four counts, what is missing, who looks
    // like someone on file, and why rows are left out. It can be downloaded.
    const dry = screen.getByTestId("dry-run");
    expect(within(dry).getByText(/Nothing has been saved yet/)).toBeInTheDocument();
    expect(within(screen.getByTestId("dry-create")).getByText("1")).toBeInTheDocument();
    expect(within(screen.getByTestId("dry-error")).getByText("Cannot be imported")).toBeVisible();
    const missing = within(screen.getByTestId("dry-missing"));
    expect(missing.getByText("Class 7")).toBeInTheDocument();
    expect(missing.getByText("Section 2-C")).toBeInTheDocument();
    expect(missing.getByText(/Settings → Academic structure/)).toBeInTheDocument();
    expect(
      within(screen.getByTestId("dry-duplicates")).getByText(
        "Row 2, Ali Raza: looks like Ali Raza (KHI-1), same father's name",
      ),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId("dry-reasons")).getByText('Class "7" is not set up in Settings'),
    ).toBeInTheDocument();
    await userEvent.click(within(dry).getByRole("button", { name: "Download dry-run report" }));
    expect(api.downloadImportReport).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "students" }),
    );
    expect(api.applyImport).not.toHaveBeenCalled();

    const bad = screen.getByTestId("preview-row-3");
    expect(within(bad).getByText(/not a date/)).toBeInTheDocument();
    const dob = within(bad).getByLabelText("Row 3: DOB");
    await userEvent.clear(dob);
    await userEvent.type(dob, "2016-05-06");
    vi.mocked(api.previewImport).mockResolvedValue({
      rows: preview.rows.map((r) => ({ ...r, errors: [], action: "create" as const })),
      summary: { total: 2, create: 2, update: 0, skip: 0, error: 0 },
      unmapped: [],
      report: { reasons: [], missing_classes: [], missing_sections: [], likely_duplicates: [] },
    });
    await userEvent.click(screen.getAllByRole("button", { name: "Check the file" })[0]!);
    await waitFor(() =>
      expect(vi.mocked(api.previewImport).mock.calls.at(-1)![0].rows[1]![2]).toBe("2016-05-06"),
    );
    // Nothing is written until the person confirms: the button asks first.
    await userEvent.click(await screen.findByRole("button", { name: "Import 2 rows" }));
    const confirm = await screen.findByRole("dialog", { name: "Import now?" });
    expect(within(confirm).getByText("2 will be created")).toBeInTheDocument();
    expect(within(confirm).getByText(/you can still undo the whole import/)).toBeInTheDocument();
    expect(api.applyImport).not.toHaveBeenCalled();
    await userEvent.click(within(confirm).getByRole("button", { name: "Import now" }));
    await waitFor(() => expect(api.applyImport).toHaveBeenCalled());
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Imported: 1 added, 0 updated, 1 skipped.",
    );
  });

  it("offers rollback in the history and builds an error file", async () => {
    vi.mocked(api.importBatches).mockResolvedValue([
      {
        id: "b1",
        school_id: "s",
        kind: "staff",
        file_name: "staff.csv",
        status: "applied",
        options: {},
        total_rows: 3,
        created_rows: 3,
        updated_rows: 0,
        skipped_rows: 0,
        applied_by: "u1",
        applied_at: "2026-10-30T10:00:00Z",
        rolled_back_at: null,
      },
    ]);
    vi.mocked(api.rollbackImport).mockResolvedValue({
      id: "b1",
      school_id: "s",
      kind: "staff",
      file_name: "staff.csv",
      status: "rolled_back",
      options: {},
      total_rows: 3,
      created_rows: 3,
      updated_rows: 0,
      skipped_rows: 0,
      applied_by: "u1",
      applied_at: "2026-10-30T10:00:00Z",
      rolled_back_at: "2026-10-30T11:00:00Z",
    });
    render();
    expect(await screen.findByText("staff.csv")).toBeInTheDocument();
    expect(screen.getByText("3 added · 0 updated · 0 skipped")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Roll back" }));
    // Nothing happens until the user confirms in the dialog.
    expect(api.rollbackImport).not.toHaveBeenCalled();
    const dialog = await screen.findByRole("dialog", { name: "Undo this import?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Roll back" }));
    await waitFor(() => expect(api.rollbackImport).toHaveBeenCalledWith("b1"));
    expect(await screen.findByText("Import rolled back.")).toBeInTheDocument();

    const csv = errorCsv(parsed.headers, parsed.rows, preview.rows);
    expect(csv.charCodeAt(0)).toBe(0xfeff);
    const lines = csv.slice(1).split("\n");
    expect(lines[0]).toBe("Student Name,Father,DOB,Sex,Class,Mobile,Problems");
    expect(lines).toHaveLength(2);
    expect(lines[1]).toContain("Sara Khan");
    expect(lines[1]).toContain("date_of_birth: Date of birth is not a date");
  });
});

describe("Import columns step", () => {
  it("shows a name built from first and last name as derived, not missing", async () => {
    render();
    await userEvent.click(await screen.findByRole("button", { name: /Students and guardians/ }));
    const file = new File(["x"], "students.xlsx", {
      type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    });
    await userEvent.upload(screen.getByLabelText("Choose a .xlsx or .csv file"), file);
    expect(await screen.findByText("2 rows found in students.xlsx")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Date of birth"), "2");
    expect(screen.queryByText("(from First name + Last name)")).not.toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Student name"), "");
    expect(screen.getByRole("button", { name: "Check the file" })).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText("First name"), "0");
    await userEvent.selectOptions(screen.getByLabelText("Last name"), "1");
    expect(screen.getByText("(from First name + Last name)")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check the file" })).toBeEnabled();
  });
});
