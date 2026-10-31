import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type { ImportField, ImportPreview, ParsedFile, UserSummary } from "@/api/types";
import { errorCsv, ImportPage } from "./ImportPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    importFields: vi.fn(),
    importBatches: vi.fn(async () => []),
    parseImport: vi.fn(),
    previewImport: vi.fn(),
    applyImport: vi.fn(),
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
});

describe("ImportPage", () => {
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
    const bad = screen.getByTestId("preview-row-3");
    expect(within(bad).getByText(/not a date/)).toBeInTheDocument();
    const dob = within(bad).getByLabelText("Row 3: DOB");
    await userEvent.clear(dob);
    await userEvent.type(dob, "2016-05-06");
    vi.mocked(api.previewImport).mockResolvedValue({
      rows: preview.rows.map((r) => ({ ...r, errors: [], action: "create" as const })),
      summary: { total: 2, create: 2, update: 0, skip: 0, error: 0 },
    });
    await userEvent.click(screen.getAllByRole("button", { name: "Check the file" })[0]!);
    await waitFor(() =>
      expect(vi.mocked(api.previewImport).mock.calls.at(-1)![0].rows[1]![2]).toBe("2016-05-06"),
    );
    await userEvent.click(await screen.findByRole("button", { name: "Import 2 rows" }));
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
