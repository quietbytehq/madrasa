import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import { memoryKeyStore, setKeyStore } from "@/lib/drafts";
import type { ImportField, ImportPreview, ParsedFile, UserSummary } from "@/api/types";
import { ImportPage } from "./ImportPage";
import { importDraft } from "./importDraft";
import { keepImport, keptImport } from "./importStore";

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

const ayesha: UserSummary = {
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
  { key: "date_of_birth", label: "Date of birth", required: true, aliases: [], example: "" },
  { key: "class", label: "Class", required: false, aliases: [], example: "1" },
];
const parsed: ParsedFile = {
  file_name: "students.xlsx",
  sheets: ["Sheet1"],
  sheet: "Sheet1",
  headers: ["Student Name", "DOB", "Class"],
  rows: [
    ["Ali Khan", "04/03/2015", "1-A"],
    ["Sara Khan", "not a date", "1-A"],
  ],
  mapping: { full_name: 0, class: 2 },
  mapping_saved: false,
};
const withError: ImportPreview = {
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
      errors: [{ field: "date_of_birth", message: "Date of birth is not a date" }],
      warnings: [],
      action: "error",
      matched: null,
    },
  ],
  summary: { total: 2, create: 1, update: 0, skip: 0, error: 1 },
  unmapped: [],
  report: { reasons: [], missing_classes: [], missing_sections: [], likely_duplicates: [] },
};
const allGood: ImportPreview = {
  ...withError,
  rows: withError.rows.map((r) => ({ ...r, errors: [], action: "create" as const })),
  summary: { total: 2, create: 2, update: 0, skip: 0, error: 0 },
};

function open(user: UserSummary = ayesha) {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions: ["students.manage"] }}>
      <ImportPage />
    </SessionProvider>,
  );
}

/** The app is closed (or the PC loses power): everything it held in memory is gone. */
function closeTheApp(view: ReturnType<typeof open>) {
  view.unmount();
  importDraft.clear();
}

const kept = () => keptImport("u1");

async function startAndMap() {
  const view = open();
  await userEvent.click(await screen.findByRole("button", { name: /Students and guardians/ }));
  await userEvent.upload(
    screen.getByLabelText("Choose a .xlsx or .csv file"),
    new File(["x"], "students.xlsx"),
  );
  await screen.findByText("2 rows found in students.xlsx");
  await userEvent.selectOptions(screen.getByLabelText("Date of birth"), "1");
  await userEvent.selectOptions(screen.getByLabelText("If a record already exists"), "update");
  return view;
}

beforeEach(() => {
  localStorage.clear();
  setKeyStore(memoryKeyStore());
  importDraft.clear();
  vi.mocked(api.importFields).mockResolvedValue(fields);
  vi.mocked(api.parseImport).mockReset();
  vi.mocked(api.parseImport).mockResolvedValue(parsed);
  vi.mocked(api.previewImport).mockReset();
  vi.mocked(api.previewImport).mockResolvedValue(withError);
  vi.mocked(api.applyImport).mockReset();
});

describe("an import that was not confirmed before the app closed", () => {
  it("offers nothing when there is none", async () => {
    open();
    expect(await screen.findByRole("button", { name: /Students and guardians/ })).toBeEnabled();
    expect(screen.queryByTestId("import-resume")).not.toBeInTheDocument();
  });

  it("is offered back with its date, and continues at the columns as they were matched", async () => {
    const view = await startAndMap();
    expect(await screen.findByTestId("import-kept")).toHaveTextContent(
      /kept on this PC until you confirm or discard it/,
    );
    await waitFor(async () => expect((await kept())?.mapping.date_of_birth).toBe(1));
    closeTheApp(view);

    open();
    const offer = await screen.findByTestId("import-resume");
    expect(screen.getByText("An import was not finished")).toBeVisible();
    expect(offer).toHaveTextContent(
      `you started importing Students and guardians from students.xlsx (2 rows)`,
    );
    expect(offer).toHaveTextContent("Nothing from this file is in the school's records yet");
    expect(within(offer).getByRole("button", { name: "Discard it" })).toBeEnabled();
    // The choice comes first: no new import is started over the old one by accident.
    expect(
      screen.queryByRole("button", { name: /Students and guardians/ }),
    ).not.toBeInTheDocument();
    // Today, as the app writes dates.
    const today = new Date().getFullYear();
    const go = within(offer).getByRole("button", {
      name: new RegExp(`^Continue the import you started on \\d{1,2}-[A-Z][a-z]{2}-${today}$`),
    });
    await userEvent.click(go);

    expect(await screen.findByText("2 rows found in students.xlsx")).toBeInTheDocument();
    expect(screen.getByLabelText("Date of birth")).toHaveValue("1");
    expect(screen.getByLabelText("Student name")).toHaveValue("0");
    expect(screen.getByLabelText("If a record already exists")).toHaveValue("update");
    // It had not been checked, so it is not checked behind the person's back either.
    expect(api.previewImport).not.toHaveBeenCalled();
    expect(api.parseImport).toHaveBeenCalledTimes(1);
  });

  it("keeps corrections made after the check, and checks the rows again when continued", async () => {
    const view = await startAndMap();
    await userEvent.click(screen.getByRole("button", { name: "Check the file" }));
    const bad = await screen.findByTestId("preview-row-3");
    const dob = within(bad).getByLabelText("Row 3: DOB");
    await userEvent.clear(dob);
    await userEvent.type(dob, "2016-05-06");
    // Closed straight after the last key: what was typed is written as the page goes.
    closeTheApp(view);
    await waitFor(async () => expect((await kept())?.edited?.[1]?.[1]).toBe("2016-05-06"));
    expect((await kept())?.step).toBe(3);

    vi.mocked(api.previewImport).mockClear();
    vi.mocked(api.previewImport).mockResolvedValue(allGood);
    open();
    const offer = await screen.findByTestId("import-resume");
    await userEvent.click(within(offer).getByRole("button", { name: /^Continue the import/ }));
    // Checked again with the corrected rows, the same columns and the same choices.
    await waitFor(() => expect(api.previewImport).toHaveBeenCalled());
    expect(api.previewImport).toHaveBeenCalledTimes(1);
    expect(vi.mocked(api.previewImport).mock.calls[0]![0]).toMatchObject({
      kind: "students",
      file_name: "students.xlsx",
      headers: parsed.headers,
      rows: [parsed.rows[0], ["Sara Khan", "2016-05-06", "1-A"]],
      mapping: { full_name: 0, class: 2, date_of_birth: 1 },
      options: { duplicates: "update" },
    });
    expect(
      await screen.findByText("2 to add, 0 to update, 0 to skip, 0 with errors"),
    ).toBeInTheDocument();
    expect(within(screen.getByTestId("preview-row-3")).getByText("2016-05-06")).toBeVisible();
    expect(
      await screen.findByText(
        "The rows were checked again against the school's records as they are now.",
      ),
    ).toBeInTheDocument();
    expect(api.parseImport).toHaveBeenCalledTimes(1);
  });

  it("stays at the columns, and is still kept, when the check cannot be made again", async () => {
    const view = await startAndMap();
    await userEvent.click(screen.getByRole("button", { name: "Check the file" }));
    await screen.findByTestId("preview-row-3");
    await waitFor(async () => expect((await kept())?.step).toBe(3));
    closeTheApp(view);

    vi.mocked(api.previewImport).mockRejectedValue(
      new Error("The school server is not answering."),
    );
    open();
    await userEvent.click(
      within(await screen.findByTestId("import-resume")).getByRole("button", {
        name: /^Continue the import/,
      }),
    );
    expect(await screen.findByText("2 rows found in students.xlsx")).toBeInTheDocument();
    expect(screen.getByLabelText("Date of birth")).toHaveValue("1");
    expect(await kept()).not.toBeNull();
  });

  it("is discarded when the person says so, and not offered again", async () => {
    closeTheApp(
      await startAndMap().then(async (v) => {
        await waitFor(async () => expect(await kept()).not.toBeNull());
        return v;
      }),
    );
    const view = open();
    await userEvent.click(
      within(await screen.findByTestId("import-resume")).getByRole("button", {
        name: "Discard it",
      }),
    );
    expect(await screen.findByText("The unfinished import was discarded.")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /Students and guardians/ })).toBeEnabled();
    await waitFor(async () => expect(await kept()).toBeNull());
    closeTheApp(view);
    open();
    expect(await screen.findByRole("button", { name: /Students and guardians/ })).toBeEnabled();
    expect(screen.queryByTestId("import-resume")).not.toBeInTheDocument();
  });

  it("is no longer kept once it has been confirmed", async () => {
    vi.mocked(api.previewImport).mockResolvedValue(allGood);
    vi.mocked(api.applyImport).mockResolvedValue({
      batch: {
        id: "b1",
        school_id: "s",
        kind: "students",
        file_name: "students.xlsx",
        status: "applied",
        options: {},
        total_rows: 2,
        created_rows: 2,
        updated_rows: 0,
        skipped_rows: 0,
        applied_by: "u1",
        applied_at: "2026-11-27T10:00:00Z",
        rolled_back_at: null,
      },
      summary: allGood.summary,
      rows: allGood.rows,
    });
    const view = await startAndMap();
    await userEvent.click(screen.getByRole("button", { name: "Check the file" }));
    await userEvent.click(await screen.findByRole("button", { name: "Import 2 rows" }));
    await waitFor(async () => expect(await kept()).not.toBeNull());
    await userEvent.click(
      within(await screen.findByRole("dialog", { name: "Import now?" })).getByRole("button", {
        name: "Import now",
      }),
    );
    expect(await screen.findByText("Imported: 2 added, 0 updated, 0 skipped.")).toBeVisible();
    await waitFor(async () => expect(await kept()).toBeNull());
    closeTheApp(view);
    open();
    expect(await screen.findByRole("button", { name: /Students and guardians/ })).toBeEnabled();
    expect(screen.queryByTestId("import-resume")).not.toBeInTheDocument();
  });

  it("is not offered to another person at the same PC", async () => {
    await keepImport({
      user: "u1",
      at: "2026-11-20T09:30:00.000Z",
      step: 2,
      kind: "students",
      parsed,
      mapping: parsed.mapping,
      remember: true,
      policy: "skip",
      edited: null,
    });
    const view = open({ ...ayesha, id: "u2", username: "bilal", full_name: "Bilal Ahmed" });
    expect(await screen.findByRole("button", { name: /Students and guardians/ })).toBeEnabled();
    expect(screen.queryByTestId("import-resume")).not.toBeInTheDocument();
    closeTheApp(view);
    // The person who started it sees the day it was started.
    open();
    const offer = await screen.findByTestId("import-resume");
    expect(
      within(offer).getByRole("button", {
        name: /^Continue the import you started on 20-Nov-2026$/,
      }),
    ).toBeEnabled();
    expect(offer).toHaveTextContent(/^On 20-Nov-2026 you started importing/);
  });

  it("says so when the import cannot be kept on this PC", async () => {
    setKeyStore({ get: () => Promise.resolve(null) });
    await startAndMap();
    expect(await screen.findByTestId("import-kept")).toHaveTextContent(
      /could not be kept on this PC/,
    );
    expect(await kept()).toBeNull();
  });
});
