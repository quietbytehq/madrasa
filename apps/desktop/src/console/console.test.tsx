import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { ApiError } from "@/api/client";
import { Gate } from "./ConsoleApp";
import { getHubToken, hub, setHubToken } from "./api";
import type { EscrowOverview, HubSchool, HubUser, Me } from "./api";
import { brief, cell, groupDigits } from "./format";
import { HubSessionContext } from "./session";
import { CorrectionsPage } from "./pages/CorrectionsPage";
import { EscrowPage } from "./pages/EscrowPage";
import { ReportsPage } from "./pages/ReportsPage";
import { SchoolsPage } from "./pages/SchoolsPage";
import { UsersPage } from "./pages/UsersPage";

vi.mock("./api", async (original) => {
  const real = await original<typeof import("./api")>();
  return {
    ...real,
    hub: Object.fromEntries(Object.keys(real.hub).map((name) => [name, vi.fn()])),
  };
});

const admin: HubUser = {
  id: "u1",
  username: "head.office",
  full_name: "Head Office",
  email: null,
  permissions: ["nexus.admin", "nexus.reports", "nexus.escrow"],
  is_active: true,
  locked_until: null,
  last_login_at: "2026-11-11T09:00:00Z",
  created_at: "2026-11-01T09:00:00Z",
};
const me: Me = { user: admin, hub_name: "Gulshan Education Network" };

const gulshan: HubSchool = {
  id: "s1",
  school_id: "school-1",
  name: "Gulshan High",
  code: "KHI-01",
  status: "active",
  school_version: "0.3.1",
  edition: "lite",
  joined_at: "2026-11-01T09:00:00Z",
  last_sync_at: "2026-11-11T10:00:00Z",
  left_at: null,
  leave_reason: null,
  purged_at: null,
  sync: { pending: 2, parked: 1, overwrites: 3 },
};
const gone: HubSchool = {
  ...gulshan,
  id: "s2",
  name: "Clifton Grammar",
  code: "KHI-02",
  status: "left",
  left_at: "2026-11-05T09:00:00Z",
  leave_reason: "Moving",
};

/** Render a page as a signed-in user holding `permissions`. */
function page(
  ui: React.ReactElement,
  permissions: string[] = admin.permissions,
  locale?: "en" | "ur",
) {
  const user = { ...admin, permissions };
  return renderWithProviders(
    <HubSessionContext.Provider
      value={{
        me: { ...me, user },
        can: (p) => permissions.includes(p),
        signOut: vi.fn(),
      }}
    >
      {ui}
    </HubSessionContext.Provider>,
    locale ? { locale } : {},
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  setHubToken(null);
  vi.mocked(hub.schools).mockResolvedValue([gulshan, gone]);
  vi.mocked(hub.invitations).mockResolvedValue([]);
  vi.mocked(hub.overwrites).mockResolvedValue([]);
  vi.mocked(hub.parked).mockResolvedValue([]);
  vi.mocked(hub.users).mockResolvedValue([admin]);
});

describe("console gate", () => {
  it("runs first-time setup, then asks to sign in", async () => {
    vi.mocked(hub.setupStatus).mockResolvedValueOnce({ needs_setup: true, hub_name: null });
    vi.mocked(hub.setupStatus).mockResolvedValue({
      needs_setup: false,
      hub_name: "Gulshan Education Network",
    });
    vi.mocked(hub.setup).mockResolvedValue({ admin_user_id: "u1" });
    renderWithProviders(<Gate />);
    const submit = await screen.findByRole("button", { name: "Finish setup" });
    expect(submit).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Setup token"), " token-from-the-log ");
    await userEvent.type(screen.getByLabelText("Name of the network"), "Gulshan Education Network");
    await userEvent.type(screen.getByLabelText("Full name"), "Head Office");
    await userEvent.type(screen.getByLabelText("Username"), "head.office");
    await userEvent.type(screen.getByLabelText("Password"), "console-password-1");
    await userEvent.type(screen.getByLabelText("Password again"), "console-password-2");
    await userEvent.click(submit);
    expect(screen.getByRole("alert")).toHaveTextContent("The two passwords do not match.");
    expect(hub.setup).not.toHaveBeenCalled();
    await userEvent.clear(screen.getByLabelText("Password again"));
    await userEvent.type(screen.getByLabelText("Password again"), "console-password-1");
    await userEvent.click(submit);
    await waitFor(() =>
      expect(hub.setup).toHaveBeenCalledWith({
        setup_token: "token-from-the-log",
        hub_name: "Gulshan Education Network",
        admin_full_name: "Head Office",
        admin_username: "head.office",
        admin_password: "console-password-1",
      }),
    );
    // Setup does not sign anyone in: the new administrator logs in like everyone else.
    expect(await screen.findByRole("button", { name: "Log in" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Gulshan Education Network" })).toBeInTheDocument();
  });

  it("signs in, shows only the pages the user may open, and signs out", async () => {
    vi.mocked(hub.setupStatus).mockResolvedValue({
      needs_setup: false,
      hub_name: "Gulshan Education Network",
    });
    vi.mocked(hub.login).mockRejectedValueOnce(
      new ApiError(401, "invalid_credentials", "The username or password is incorrect."),
    );
    vi.mocked(hub.login).mockResolvedValue({ token: "session-token", user: admin });
    vi.mocked(hub.me).mockResolvedValue({
      ...me,
      user: { ...admin, permissions: ["nexus.reports"] },
    });
    vi.mocked(hub.logout).mockResolvedValue(undefined);
    vi.mocked(hub.reports).mockResolvedValue([]);
    renderWithProviders(<Gate />);
    await userEvent.type(await screen.findByLabelText("Username"), "head.office");
    await userEvent.type(screen.getByLabelText("Password"), "wrong");
    await userEvent.click(screen.getByRole("button", { name: "Log in" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The username or password is incorrect.",
    );
    expect(getHubToken()).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Log in" }));
    const nav = await screen.findByRole("navigation");
    expect(getHubToken()).toBe("session-token");
    // A report reader sees schools and reports; nothing that needs more.
    expect(
      within(nav)
        .getAllByRole("link")
        .map((a) => a.textContent),
    ).toEqual(["Schools", "Reports"]);
    expect(screen.queryByRole("button", { name: "Invite a school" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Log out" }));
    expect(await screen.findByRole("button", { name: "Log in" })).toBeInTheDocument();
    expect(getHubToken()).toBeNull();
    expect(hub.logout).toHaveBeenCalled();
  });

  it("says so when the hub cannot be reached, and tries again", async () => {
    vi.mocked(hub.setupStatus).mockRejectedValueOnce(new Error("network"));
    vi.mocked(hub.setupStatus).mockResolvedValue({ needs_setup: false, hub_name: null });
    renderWithProviders(<Gate />);
    expect(await screen.findByRole("alert")).toHaveTextContent("The hub could not be reached.");
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("button", { name: "Log in" })).toBeInTheDocument();
  });
});

describe("schools", () => {
  it("lists schools with their sync state and shows a new join code once", async () => {
    vi.mocked(hub.invite).mockResolvedValue({
      id: "i1",
      school_name: "Clifton Grammar",
      note: null,
      expires_at: "2026-11-18T09:00:00Z",
      used_at: null,
      revoked_at: null,
      created_at: "2026-11-11T09:00:00Z",
      join_code: "ABCD-EFGH-JKMN-PQRS",
      hub_url: "https://nexus.example.org",
    });
    page(<SchoolsPage />);
    const row = (await screen.findByText("Gulshan High")).closest("tr")!;
    expect(within(row).getByText("Joined")).toBeInTheDocument();
    expect(within(row).getByText("2 to send · 1 held · 3 overwritten")).toBeInTheDocument();
    expect(
      within(screen.getByText("Clifton Grammar").closest("tr")!).getByText("Left"),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Invite a school" }));
    await userEvent.type(screen.getByLabelText("School's name"), "Clifton Grammar");
    await userEvent.click(screen.getByRole("button", { name: "Create join code" }));
    await waitFor(() => expect(hub.invite).toHaveBeenCalledWith("Clifton Grammar", ""));
    expect(await screen.findByTestId("join-code")).toHaveTextContent("ABCD-EFGH-JKMN-PQRS");
    expect(screen.getByText("https://nexus.example.org")).toBeInTheDocument();
    expect(screen.getByText(/shown only this once/)).toBeInTheDocument();
  });

  it("removes a school only with a reason, and deletes an archive only with its code", async () => {
    vi.mocked(hub.removeSchool).mockResolvedValue({ ...gulshan, status: "removed" });
    vi.mocked(hub.purgeSchool).mockResolvedValue({ ...gone, status: "purged" });
    vi.mocked(hub.overwrites).mockResolvedValue([
      {
        uid: "o1",
        at: "2026-11-11T10:00:00Z",
        table: "students",
        id: "x",
        field: "full_name",
        kept: "Sara (head office)",
        discarded: "Sara (school)",
        kept_origin: "head_office",
        reason: "head_office_wins",
      },
    ]);
    page(<SchoolsPage />);
    const open = await screen.findAllByRole("button", { name: "Open" });
    await userEvent.click(open[0]!);
    let dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText("Sara (school)")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove from the network" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("It keeps all its own records.");
    const remove = within(dialog).getByRole("button", { name: "Remove from the network" });
    expect(remove).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Reason"), "The school closed");
    await userEvent.click(remove);
    await waitFor(() => expect(hub.removeSchool).toHaveBeenCalledWith("s1", "The school closed"));

    await userEvent.click((await screen.findAllByRole("button", { name: "Open" }))[1]!);
    dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete the archive" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("It cannot be undone.");
    const purge = within(dialog).getByRole("button", { name: "Delete the archive" });
    await userEvent.type(
      within(dialog).getByLabelText(/Type the school's code \(KHI-02\)/),
      "KHI-01",
    );
    expect(purge).toBeDisabled();
    await userEvent.clear(within(dialog).getByLabelText(/Type the school's code/));
    await userEvent.type(within(dialog).getByLabelText(/Type the school's code/), "khi-02");
    await userEvent.click(purge);
    await waitFor(() => expect(hub.purgeSchool).toHaveBeenCalledWith("s2", "khi-02"));
  });

  it("gives a report reader the list and nothing else", async () => {
    page(<SchoolsPage />, ["nexus.reports"]);
    expect(await screen.findByText("Gulshan High")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Invite a school" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open" })).not.toBeInTheDocument();
    expect(hub.invitations).not.toHaveBeenCalled();
  });
});

describe("reports", () => {
  it("runs a report for the chosen school and period and shows totals; downloads use the same choices", async () => {
    vi.mocked(hub.reports).mockResolvedValue([
      {
        code: "summary",
        title: "Network summary",
        about: "One row per school.",
        takes_period: false,
      },
      {
        code: "fees",
        title: "Fees by school and month",
        about: "Billed and collected.",
        takes_period: true,
      },
    ]);
    vi.mocked(hub.report).mockResolvedValue({
      code: "fees",
      title: "Fees by school and month",
      about: "",
      generated_at: "2026-11-11T10:00:00Z",
      from: "2026-04-01",
      to: "2026-10-31",
      schools: 1,
      columns: [
        { key: "school", label: "School", kind: "text" },
        { key: "billed", label: "Billed", kind: "money" },
        { key: "rate", label: "Collected %", kind: "percent" },
      ],
      rows: [["Gulshan High", "10497123.00", "63.9"]],
      totals: [["Total", "10497123.00", null]],
      notes: ["Money is never added across currencies."],
    });
    vi.mocked(hub.downloadReport).mockResolvedValue(undefined);
    page(<ReportsPage />);
    const show = await screen.findByRole("button", { name: "Show" });
    expect(show).toBeDisabled();
    expect(hub.reports).toHaveBeenCalledWith("en");
    await userEvent.selectOptions(screen.getByLabelText("Report"), "fees");
    // Only joined schools can be chosen.
    const schools = within(screen.getByLabelText("School")).getAllByRole("option");
    expect(schools.map((o) => o.textContent)).toEqual([
      "All joined schools",
      "Gulshan High (KHI-01)",
    ]);
    await userEvent.selectOptions(screen.getByLabelText("School"), "s1");
    await userEvent.type(screen.getByLabelText("From"), "2026-04-01");
    await userEvent.click(show);
    const expected = { lang: "en", from: "2026-04-01", school_id: "s1" };
    await waitFor(() => expect(hub.report).toHaveBeenCalledWith("fees", expected));
    const table = await screen.findByRole("table");
    // The amount is shown digit for digit, only grouped.
    expect(within(table).getAllByText("10,497,123.00")).toHaveLength(2);
    expect(within(table).getByText("63.9%")).toBeInTheDocument();
    expect(screen.getByText("Money is never added across currencies.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Download Excel" }));
    await waitFor(() => expect(hub.downloadReport).toHaveBeenCalledWith("fees", expected, "xlsx"));
  });

  it("asks for the catalogue in Urdu when the console is in Urdu", async () => {
    vi.mocked(hub.reports).mockResolvedValue([]);
    page(<ReportsPage />, admin.permissions, "ur");
    await waitFor(() => expect(hub.reports).toHaveBeenCalledWith("ur"));
    expect(screen.getByRole("heading", { name: "رپورٹس" })).toBeInTheDocument();
  });
});

describe("corrections", () => {
  it("finds a record and sends only the field that was changed", async () => {
    vi.mocked(hub.correctionKinds).mockResolvedValue([
      { code: "school", fields: [{ name: "name", required: true }] },
      {
        code: "students",
        fields: [
          { name: "full_name", required: true },
          { name: "father_name", required: true },
          { name: "address", required: false },
        ],
      },
    ]);
    const record = {
      id: "r1",
      title: "Ali Khan",
      hint: "S-0001",
      fields: { full_name: "Ali Khan", father_name: "Ahmed", address: "" },
    };
    vi.mocked(hub.records).mockResolvedValue({ items: [record], total: 1, page: 1, page_size: 25 });
    vi.mocked(hub.correct).mockResolvedValue({ record, changed: ["full_name"] });
    page(<CorrectionsPage />);
    // What a correction does is said on the page, before anything is changed.
    expect(await screen.findByText(/head office's value is kept/)).toBeInTheDocument();
    const search = screen.getByRole("button", { name: "Search" });
    expect(search).toBeDisabled();
    await userEvent.selectOptions(await screen.findByLabelText("School"), "s1");
    await userEvent.type(screen.getByLabelText("Name or number"), "ali");
    await userEvent.click(search);
    await waitFor(() => expect(hub.records).toHaveBeenCalledWith("s1", "students", "ali", 1));
    expect(await screen.findByText("S-0001")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Correct" }));
    const dialog = await screen.findByRole("dialog");
    const save = within(dialog).getByRole("button", { name: "Save correction" });
    expect(save).toBeDisabled();
    const name = within(dialog).getByLabelText("Full name");
    await userEvent.clear(name);
    expect(save).toBeDisabled();
    await userEvent.type(name, "Ali Ahmed Khan");
    await userEvent.click(save);
    await waitFor(() =>
      expect(hub.correct).toHaveBeenCalledWith("s1", "students", "r1", {
        full_name: "Ali Ahmed Khan",
      }),
    );
    expect(
      await screen.findByText(/The school will receive it at its next sync/),
    ).toBeInTheDocument();
  });
});

describe("backup keys", () => {
  const overview: EscrowOverview = {
    set_up: true,
    deposits: [
      {
        school_id: "s1",
        school_name: "Gulshan High",
        school_code: "KHI-01",
        key_id: "k",
        deposited_at: "2026-11-02T09:00:00Z",
      },
    ],
    releases: [
      {
        id: "r0",
        school_id: "s1",
        school_name: "Gulshan High",
        school_code: "KHI-01",
        released_by: "Head Office",
        reason: "Earlier",
        released_at: "2026-11-03T09:00:00Z",
        seen_at: null,
      },
    ],
  };

  it("sets escrow up with a long passphrase typed twice", async () => {
    vi.mocked(hub.escrow).mockResolvedValue({ set_up: false, deposits: [], releases: [] });
    vi.mocked(hub.escrowSetup).mockResolvedValue(undefined);
    page(<EscrowPage />);
    expect(await screen.findByRole("note")).toHaveTextContent(
      "if it is lost, no key left here can ever be released",
    );
    const submit = screen.getByRole("button", { name: "Set up key escrow" });
    await userEvent.type(screen.getByLabelText("Escrow passphrase"), "short");
    await userEvent.type(screen.getByLabelText("Passphrase again"), "short");
    expect(submit).toBeDisabled();
    await userEvent.clear(screen.getByLabelText("Escrow passphrase"));
    await userEvent.type(screen.getByLabelText("Escrow passphrase"), "a long passphrase");
    expect(screen.getByText("The two passphrases do not match.")).toBeInTheDocument();
    await userEvent.clear(screen.getByLabelText("Passphrase again"));
    await userEvent.type(screen.getByLabelText("Passphrase again"), "a long passphrase");
    await userEvent.click(submit);
    await waitFor(() => expect(hub.escrowSetup).toHaveBeenCalledWith("a long passphrase"));
  });

  it("releases a key with a reason and the passphrase and shows it once", async () => {
    vi.mocked(hub.escrow).mockResolvedValue(overview);
    vi.mocked(hub.releaseKey).mockResolvedValue({
      release_id: "r1",
      school_name: "Gulshan High",
      school_code: "KHI-01",
      released_at: "2026-11-11T10:00:00Z",
      released_key: "0123A-BCDEF-GHJKM",
    });
    page(<EscrowPage />);
    // A release the school has not been told about yet stands out.
    expect(await screen.findByText("Not yet")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Release key" }));
    let dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("the school sees it after its next sync");
    const release = within(dialog).getByRole("button", { name: "Release key" });
    await userEvent.type(within(dialog).getByLabelText("Escrow passphrase"), "a long passphrase");
    expect(release).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Reason"), "Principal lost the password");
    await userEvent.click(release);
    await waitFor(() =>
      expect(hub.releaseKey).toHaveBeenCalledWith(
        "s1",
        "a long passphrase",
        "Principal lost the password",
      ),
    );
    dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByTestId("released-key")).toHaveTextContent("0123A-BCDEF-GHJKM");
    expect(dialog).toHaveTextContent("shown only this once and is not kept here");
    await userEvent.click(within(dialog).getByRole("button", { name: "I have copied it" }));
    await waitFor(() => expect(screen.queryByText("0123A-BCDEF-GHJKM")).not.toBeInTheDocument());
  });

  it("lets an administrator without the release permission see the list but not release", async () => {
    vi.mocked(hub.escrow).mockResolvedValue(overview);
    page(<EscrowPage />, ["nexus.admin"]);
    expect(await screen.findByText("Gulshan High", { selector: "div" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Release key" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Change the passphrase" })).not.toBeInTheDocument();
  });
});

describe("users", () => {
  it("adds a user with chosen permissions", async () => {
    vi.mocked(hub.createUser).mockResolvedValue({ ...admin, id: "u2" });
    page(<UsersPage />);
    expect(await screen.findByText("(you)")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Add a user" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Full name"), "Sana Malik");
    await userEvent.type(within(dialog).getByLabelText("Username"), "sana");
    await userEvent.type(within(dialog).getByLabelText("Password"), "viewer-password-1");
    // Reading reports is offered by default; releasing keys has to be chosen.
    expect(within(dialog).getByRole("checkbox", { name: /Read reports/ })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /Release backup keys/ })).not.toBeChecked();
    await userEvent.click(within(dialog).getByRole("checkbox", { name: /Release backup keys/ }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(hub.createUser).toHaveBeenCalledWith({
        username: "sana",
        full_name: "Sana Malik",
        email: null,
        password: "viewer-password-1",
        permissions: ["nexus.reports", "nexus.escrow"],
      }),
    );
  });
});

describe("formatting", () => {
  it("groups money as text and never rounds it", () => {
    expect(groupDigits("10497123.00")).toBe("10,497,123.00");
    expect(groupDigits("-1234567.50")).toBe("-1,234,567.50");
    expect(groupDigits("999.99")).toBe("999.99");
    expect(groupDigits("12345678901234.56")).toBe("12,345,678,901,234.56");
    expect(groupDigits("n/a")).toBe("n/a");
    const i18n = { formatNumber: (n: number) => String(n), formatDate: () => "a date" };
    expect(cell("0.10", "money", i18n)).toBe("0.10");
    expect(cell(null, "money", i18n)).toBe("—");
    expect(cell(12, "int", i18n)).toBe("12");
    expect(cell("2026-04-01", "date", i18n)).toBe("a date");
    expect(brief({ reason: "x".repeat(300) }).length).toBe(158);
    expect(brief(null)).toBe("");
  });
});
