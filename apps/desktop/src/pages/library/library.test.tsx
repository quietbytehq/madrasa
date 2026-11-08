import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import type { BookView, CopyLookup, LoanView, MemberPage, UserSummary } from "@/api/types";
import { LibraryPage } from "./LibraryPage";

vi.mock("@/api/endpoints", () => ({
  api: {
    lookupCopy: vi.fn(),
    libraryMembers: vi.fn(),
    libraryMember: vi.fn(),
    issueBook: vi.fn(),
    returnLoan: vi.fn(),
    renewLoan: vi.fn(),
    settleFine: vi.fn(),
    waiveFine: vi.fn(),
    libraryBooks: vi.fn(),
    libraryBook: vi.fn(),
    createBook: vi.fn(),
    libraryLoans: vi.fn(),
    libraryStock: vi.fn(),
    libraryCopies: vi.fn(),
    librarySettings: vi.fn(),
    feeHeads: vi.fn(),
  },
}));

const admin: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "lib",
  full_name: "Zara Qureshi",
  email: null,
  is_active: true,
  roles: ["librarian"],
};

const loan = (over: Partial<LoanView>): LoanView => ({
  id: "l1",
  school_id: "s",
  copy_id: "c1",
  member_kind: "student",
  member_id: "st1",
  issued_on: "2026-04-01",
  due_on: "2026-04-15",
  returned_on: null,
  renewals: 0,
  issued_by: "u1",
  returned_by: null,
  lost: false,
  fine: "0.00",
  fine_waived: "0.00",
  waiver_reason: "",
  waived_by: null,
  fine_voucher_id: null,
  fine_settled_on: null,
  hlc: "",
  accession_no: "A-001",
  title: "Physics",
  author: "Halliday",
  member_name: "Ali Khan",
  member_number: "2026-0001",
  member_detail: "9-A",
  ...over,
});

const freeCopy: CopyLookup = {
  id: "c2",
  school_id: "s",
  book_id: "b1",
  accession_no: "A-002",
  shelf: "S1",
  condition: "good",
  status: "available",
  hlc: "",
  title: "Physics",
  author: "Halliday",
  category: "Science",
  loan: null,
};

const memberPage = (loans: LoanView[]): MemberPage => ({
  member: { kind: "student", id: "st1", name: "Ali Khan", number: "2026-0001", detail: "9-A" },
  loans,
  open: loans.filter((l) => !l.returned_on).length,
  overdue: 0,
  fines_due: "0.00",
  rules: { max_books: 2, loan_days: 14, fine_per_day: "5", max_renewals: 1 },
});

function renderPage(permissions: string[]) {
  return renderWithProviders(
    <FeaturesProvider initial={{ edition: "standard", features: ["dashboard", "library"] }}>
      <SessionProvider initial={{ user: admin, permissions }}>
        <LibraryPage />
      </SessionProvider>
    </FeaturesProvider>,
    { route: "/library" },
  );
}

describe("LibraryPage", () => {
  beforeEach(() => {
    vi.mocked(api.libraryMembers).mockResolvedValue([
      { kind: "student", id: "st1", name: "Ali Khan", number: "2026-0001", detail: "9-A" },
    ]);
    vi.mocked(api.libraryMember).mockResolvedValue(memberPage([]));
  });

  it("issues a free copy to the picked member", async () => {
    vi.mocked(api.lookupCopy).mockResolvedValue(freeCopy);
    vi.mocked(api.issueBook).mockResolvedValue(
      memberPage([loan({ copy_id: "c2", accession_no: "A-002" })]),
    );
    renderPage(["library.read", "library.manage"]);
    await userEvent.type(screen.getByLabelText("Search students and staff"), "Ali");
    await userEvent.click(
      await within(await screen.findByTestId("members")).findByText("Ali Khan"),
    );
    expect(await screen.findByTestId("member")).toHaveTextContent("0 out");
    await userEvent.type(screen.getByLabelText("Accession number"), "A-002");
    await userEvent.click(screen.getByRole("button", { name: "Look up" }));
    expect(await screen.findByTestId("copy")).toHaveTextContent("Physics");
    await userEvent.click(screen.getByTestId("issue"));
    await waitFor(() =>
      expect(api.issueBook).toHaveBeenCalledWith({
        copy_id: "c2",
        member_kind: "student",
        member_id: "st1",
      }),
    );
  });

  it("returns a copy that is out and shows the overdue state", async () => {
    const out = loan({ due_on: "2020-01-01" });
    vi.mocked(api.lookupCopy).mockResolvedValue({
      ...freeCopy,
      id: "c1",
      accession_no: "A-001",
      status: "on_loan",
      loan: out,
    });
    vi.mocked(api.returnLoan).mockResolvedValue(loan({ returned_on: "2026-04-20", fine: "25.00" }));
    renderPage(["library.read", "library.manage"]);
    await userEvent.type(screen.getByLabelText("Accession number"), "A-001");
    await userEvent.click(screen.getByRole("button", { name: "Look up" }));
    const copy = await screen.findByTestId("copy");
    expect(copy).toHaveTextContent("Ali Khan");
    expect(copy).toHaveTextContent("Overdue");
    await userEvent.click(screen.getByTestId("return"));
    await waitFor(() => expect(api.returnLoan).toHaveBeenCalledWith("l1", {}));
  });

  it("lists the catalogue and hides the settings tab from readers", async () => {
    const book: BookView = {
      id: "b1",
      school_id: "s",
      title: "Physics",
      author: "Halliday",
      publisher: "",
      isbn: "",
      category: "Science",
      language: "English",
      subject: "",
      year: null,
      custom: {},
      hlc: "",
      copies: 3,
      available: 2,
      on_loan: 1,
    };
    vi.mocked(api.libraryBooks).mockResolvedValue([book]);
    renderPage(["library.read"]);
    expect(screen.getAllByRole("tab")).toHaveLength(4);
    await userEvent.click(screen.getByRole("tab", { name: "Catalogue" }));
    expect(await screen.findByText("Physics")).toBeInTheDocument();
    expect(screen.queryByTestId("add-book")).not.toBeInTheDocument();
  });
});
