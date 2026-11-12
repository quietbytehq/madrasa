import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import type { ReportCard } from "@/api/types";
import type { Locale } from "@/i18n";
import { ParentApp } from "./App";
import { HubError, OfflineError, getToken, hubApi } from "./api";
import type {
  AttendanceMonth,
  Child,
  Conversation,
  FeeStatus,
  Notices,
  Overview,
  ParentSession,
} from "./api";

vi.mock("./api", async (original) => {
  const actual = await original<typeof import("./api")>();
  return {
    ...actual,
    hubApi: {
      checkCode: vi.fn(),
      activate: vi.fn(),
      login: vi.fn(),
      logout: vi.fn(),
      me: vi.fn(),
      children: vi.fn(),
      overview: vi.fn(),
      attendance: vi.fn(),
      results: vi.fn(),
      fees: vi.fn(),
      notices: vi.fn(),
      messages: vi.fn(),
      sendMessage: vi.fn(),
      markRead: vi.fn(),
    },
  };
});

const HUB = "https://hub.example.org";
const NOW = "2026-11-12T08:00:00Z";
const REF = { school: "sch1", student: "ali" };
const CHILD = "/child/sch1/ali";

const session: ParentSession = {
  token: "tok-1",
  expires_at: "2026-12-12T08:00:00Z",
  parent: { id: "p1", phone: "+923001111111", full_name: "Imran Khan", last_login_at: null },
};

const ali: Child = {
  school_ref: "sch1",
  school_name: "Gulshan High",
  synced_at: NOW,
  student_id: "ali",
  full_name: "Ali Khan",
  student_number: "KHI-2026-00001",
  status: "active",
  relationship: "father",
  class_name: "5",
  section_name: "A",
  roll_no: 7,
  unread: 2,
};

const overview: Overview = {
  ...ali,
  class_teacher: "Sana Malik",
  academic_year: "2026-27",
  attendance: { present: 40, absent: 3, late: 2, leave: 1, total: 46, percent: 91.3 },
  currency: "PKR",
  balance: "4500.00",
  open_vouchers: 1,
  unread: 2,
  notices: 3,
};

const attendance = (month: string): AttendanceMonth => ({
  synced_at: NOW,
  month,
  days: [
    { date: `${month}-03`, status: "present" },
    { date: `${month}-04`, status: "absent" },
    { date: `${month}-05`, status: "late" },
  ],
  summary: { present: 1, absent: 1, late: 1, leave: 0, total: 3, percent: 66.7 },
  year: {
    name: "2026-27",
    start_date: "2026-04-01",
    end_date: "2027-03-31",
    summary: { present: 40, absent: 3, late: 2, leave: 1, total: 46, percent: 91.3 },
  },
});

const empty = { max_marks: null, obtained: null, is_absent: false, percent: null, approved: false };
/** English approved; Mathematics entered at the school but not approved, so not sent. */
const card: ReportCard = {
  school: { name: "Gulshan High", code: "KHI", city: "Karachi" },
  academic_year: "2026-27",
  student_id: "ali",
  student_number: "KHI-2026-00001",
  full_name: "Ali Khan",
  father_name: "Imran Khan",
  class_name: "5",
  section_name: "A",
  roll_no: 7,
  assessments: [
    {
      id: "a1",
      name: "Bi-monthly Test 1",
      weight: "15.00",
      held: true,
      term_id: "t1",
      kind: "entered",
    },
    { id: "a2", name: "Mid-year", weight: "30.00", held: false, term_id: "t1", kind: "entered" },
  ],
  terms: [
    {
      id: "t1",
      name: "Year",
      weight: "100.00",
      component_ids: ["a1", "a2"],
      percent: "90.00",
      grade: "A+",
    },
  ],
  subjects: [
    {
      subject_id: "eng",
      subject_name: "English",
      cells: [
        {
          max_marks: "50.00",
          obtained: "45.00",
          is_absent: false,
          percent: "90.00",
          approved: true,
        },
        empty,
      ],
      term_percents: ["90.00"],
      weighted_percent: "90.00",
      grade: "A+",
    },
    {
      subject_id: "mth",
      subject_name: "Mathematics",
      cells: [empty, empty],
      term_percents: [null],
      weighted_percent: null,
      grade: null,
    },
  ],
  totals: [
    {
      assessment_id: "a1",
      total: "45.00",
      max: "50.00",
      percent: "90.00",
      grade: "A+",
      position: null,
      students: 0,
    },
    {
      assessment_id: "a2",
      total: "0.00",
      max: "0.00",
      percent: null,
      grade: null,
      position: null,
      students: 0,
    },
  ],
  overall_percent: "90.00",
  overall_grade: "A+",
  overall_remarks: "Outstanding",
  pass: true,
  attendance: { present: 42, total: 46, percent: 91.3 },
  grading: { pass_percent: "40", bands: [] },
} as ReportCard;

const fees: FeeStatus = {
  synced_at: NOW,
  currency: "PKR",
  balance: "4500.00",
  vouchers: [
    {
      id: "v2",
      voucher_no: "V202605-00001",
      billing_month: "2026-05-01",
      issue_date: "2026-05-01",
      due_date: "2026-05-10",
      lines: [{ description: "Tuition", amount: "2500.00" }],
      gross: "2500.00",
      sibling_discount: "0.00",
      concession_discount: "0.00",
      net_monthly: "2500.00",
      arrears: "2750.00",
      total_due: "5250.00",
      late_fee: "250.00",
      net_paid: "1000.00",
      balance: "4500.00",
      status: "partial",
    },
    {
      id: "v1",
      voucher_no: "V202604-00001",
      billing_month: "2026-04-01",
      issue_date: "2026-04-01",
      due_date: "2026-04-10",
      lines: [{ description: "Tuition", amount: "2500.00" }],
      gross: "2500.00",
      sibling_discount: "0.00",
      concession_discount: "0.00",
      net_monthly: "2500.00",
      arrears: "0.00",
      total_due: "2500.00",
      late_fee: "250.00",
      net_paid: "0.00",
      balance: "0",
      status: "carried",
    },
  ],
  receipts: [
    {
      id: "r1",
      receipt_no: "R-00001",
      voucher_no: "V202605-00001",
      paid_on: "2026-05-12",
      amount: "1000.00",
      method: "cash",
      reversal: false,
    },
  ],
};

const notices: Notices = {
  synced_at: NOW,
  notices: [
    {
      id: "n2",
      title: "Class 5 trip",
      body: "Permission slips by Monday.",
      published_at: "2026-11-11T08:00:00Z",
      expires_on: null,
      whole_school: false,
    },
    {
      id: "n1",
      title: "Sports day",
      body: "Friday.\nBring water.",
      published_at: "2026-11-10T08:00:00Z",
      expires_on: null,
      whole_school: true,
    },
  ],
};

const thread: Conversation = {
  synced_at: NOW,
  class_teacher: "Sana Malik",
  class_name: "5",
  section_name: "A",
  can_send: true,
  unread: 1,
  messages: [
    {
      id: "m1",
      sender: "guardian",
      sender_name: "Imran Khan",
      body: "Ali has a fever.",
      sent_at: NOW,
    },
    { id: "m2", sender: "staff", sender_name: "Sana Malik", body: "Thank you.", sent_at: NOW },
  ],
};

function signedIn() {
  localStorage.setItem("madrasa.parent.hub", HUB);
  localStorage.setItem("madrasa.parent.session", "tok-1");
  localStorage.setItem("madrasa.parent.name", "Imran Khan");
  localStorage.setItem("madrasa.parent.phone", "+923001111111");
}

function open(route = "/", locale: Locale = "en") {
  return render(
    <MemoryRouter initialEntries={[route]}>
      <ParentApp initialLocale={locale} />
    </MemoryRouter>,
  );
}

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-12T09:00:00Z") });
  vi.mocked(hubApi.children).mockResolvedValue({ children: [ali] });
  vi.mocked(hubApi.overview).mockResolvedValue(overview);
  vi.mocked(hubApi.attendance).mockImplementation((_c, month) =>
    Promise.resolve(attendance(month ?? "2026-11")),
  );
  vi.mocked(hubApi.results).mockResolvedValue({ synced_at: NOW, card });
  vi.mocked(hubApi.fees).mockResolvedValue(fees);
  vi.mocked(hubApi.notices).mockResolvedValue(notices);
  vi.mocked(hubApi.messages).mockResolvedValue(thread);
  vi.mocked(hubApi.markRead).mockResolvedValue({ unread: 0 });
  vi.mocked(hubApi.logout).mockResolvedValue(null);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("before signing in", () => {
  it("opens an account with the access code from the school's slip", async () => {
    vi.mocked(hubApi.checkCode).mockResolvedValue({
      guardian_name: "Imran Khan",
      school_name: "Gulshan High",
      children: ["Ali Khan", "Sara Khan"],
      existing_phone_hint: null,
    });
    vi.mocked(hubApi.activate).mockResolvedValue(session);
    open();
    await userEvent.click(await screen.findByRole("link", { name: "I have an access code" }));

    // An address that is not one is refused before anything is sent.
    await userEvent.type(screen.getByTestId("hub-address"), "my school");
    await userEvent.type(screen.getByTestId("access-code"), "abcd-efgh-jkmn");
    await userEvent.click(screen.getByTestId("check-code"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "does not look like a school network",
    );
    expect(hubApi.checkCode).not.toHaveBeenCalled();

    await userEvent.clear(screen.getByTestId("hub-address"));
    await userEvent.type(screen.getByTestId("hub-address"), "hub.example.org/ ");
    await userEvent.click(screen.getByTestId("check-code"));
    await waitFor(() => expect(hubApi.checkCode).toHaveBeenCalledWith(HUB, "ABCD-EFGH-JKMN"));

    // The parent sees whose code it is before choosing a password.
    expect(await screen.findByRole("heading", { name: "Hello, Imran Khan" })).toBeInTheDocument();
    const whose = screen.getByTestId("code-for");
    expect(within(whose).getByText("Gulshan High")).toBeInTheDocument();
    expect(within(whose).getByText("Sara Khan")).toBeInTheDocument();

    await userEvent.type(screen.getByTestId("phone"), "0300 1111111");
    await userEvent.type(screen.getByTestId("password"), "parent-password-1");
    await userEvent.type(screen.getByTestId("password-again"), "parent-password-2");
    await userEvent.click(screen.getByTestId("activate"));
    expect(await screen.findByRole("alert")).toHaveTextContent("not the same");
    expect(hubApi.activate).not.toHaveBeenCalled();
    await userEvent.clear(screen.getByTestId("password-again"));
    await userEvent.type(screen.getByTestId("password-again"), "parent-password-1");
    await userEvent.click(screen.getByTestId("activate"));
    await waitFor(() =>
      expect(hubApi.activate).toHaveBeenCalledWith(
        HUB,
        "ABCD-EFGH-JKMN",
        "0300 1111111",
        "parent-password-1",
      ),
    );

    // Signed in: the children, and the session is kept for next time.
    expect(await screen.findByRole("heading", { name: "My children" })).toBeInTheDocument();
    expect(getToken()).toBe("tok-1");
    expect(localStorage.getItem("madrasa.parent.hub")).toBe(HUB);
  });

  it("says in plain words why a code or a password was refused", async () => {
    vi.mocked(hubApi.checkCode).mockRejectedValue(new HubError(403, "bad_access_code", "No."));
    open("/code");
    await userEvent.type(await screen.findByTestId("hub-address"), HUB);
    await userEvent.type(screen.getByTestId("access-code"), "ZZZZ-ZZZZ-ZZZZ");
    await userEvent.click(screen.getByTestId("check-code"));
    expect(await screen.findByRole("alert")).toHaveTextContent("This access code does not work");
  });

  it("signs in with phone and password, and explains a wrong one", async () => {
    vi.mocked(hubApi.login)
      .mockRejectedValueOnce(new HubError(401, "invalid_credentials", "Wrong."))
      .mockResolvedValueOnce(session);
    open("/login");
    await userEvent.type(await screen.findByTestId("hub-address"), "hub.example.org");
    await userEvent.type(screen.getByTestId("phone"), "0300 1111111");
    await userEvent.type(screen.getByTestId("password"), "wrong");
    await userEvent.click(screen.getByTestId("sign-in"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The phone number or password is not right.",
    );
    await userEvent.clear(screen.getByTestId("password"));
    await userEvent.type(screen.getByTestId("password"), "parent-password-1");
    await userEvent.click(screen.getByTestId("sign-in"));
    await waitFor(() =>
      expect(hubApi.login).toHaveBeenLastCalledWith(HUB, "0300 1111111", "parent-password-1"),
    );
    expect(await screen.findByTestId("child-ali")).toBeInTheDocument();
  });

  it("says so when there is no connection", async () => {
    vi.mocked(hubApi.login).mockRejectedValue(new OfflineError());
    open("/login");
    await userEvent.type(await screen.findByTestId("hub-address"), HUB);
    await userEvent.type(screen.getByTestId("phone"), "0300");
    await userEvent.type(screen.getByTestId("password"), "x");
    await userEvent.click(screen.getByTestId("sign-in"));
    expect(await screen.findByRole("alert")).toHaveTextContent("could not be reached");
  });

  it("takes a phone that was set up before straight to sign-in, address filled in", async () => {
    localStorage.setItem("madrasa.parent.hub", HUB);
    localStorage.setItem("madrasa.parent.phone", "+923001111111");
    open("/child/sch1/ali/fees");
    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.getByTestId("hub-address")).toHaveValue(HUB);
    expect(screen.getByTestId("phone")).toHaveValue("+923001111111");
    expect(hubApi.fees).not.toHaveBeenCalled();
  });
});

describe("signed in", () => {
  beforeEach(signedIn);

  it("lists the children with their class, school and unread messages", async () => {
    open();
    const link = await screen.findByTestId("child-ali");
    expect(link).toHaveAttribute("href", CHILD);
    expect(within(link).getByText("Ali Khan")).toBeInTheDocument();
    expect(link).toHaveTextContent("Class 5-A · Gulshan High");
    expect(within(link).getByLabelText("2 new")).toBeInTheDocument();
    expect(screen.getByTestId("last-updated")).toHaveTextContent("Last updated by the school");
    expect(screen.getByText("Imran Khan")).toBeInTheDocument();
  });

  it("explains an empty list instead of showing nothing", async () => {
    vi.mocked(hubApi.children).mockResolvedValue({ children: [] });
    open();
    expect(await screen.findByTestId("no-children")).toHaveTextContent("No children to show");
  });

  it("shows a child at a glance, each line leading to its section", async () => {
    open(CHILD);
    expect(await screen.findByRole("heading", { name: "Ali Khan" })).toBeInTheDocument();
    expect(hubApi.overview).toHaveBeenCalledWith(REF);
    expect(await screen.findByTestId("tile-attendance")).toHaveTextContent("91.3%");
    expect(screen.getByTestId("tile-attendance")).toHaveTextContent("42 of 46 days");
    expect(screen.getByTestId("tile-fees")).toHaveTextContent("PKR 4,500.00");
    expect(screen.getByTestId("tile-fees")).toHaveTextContent("1 unpaid voucher");
    expect(screen.getByTestId("tile-fees")).not.toHaveTextContent("vouchers");
    expect(screen.getByTestId("tile-notices")).toHaveTextContent("3 to read");
    expect(screen.getByTestId("tile-messages")).toHaveTextContent("2 new");
    expect(screen.getByText("Sana Malik")).toBeInTheDocument();
    expect(screen.getByTestId("nav-unread")).toHaveTextContent("2");
    // Six sections, by name, for the thumb.
    const nav = screen.getByRole("navigation", { name: "Sections" });
    expect(
      within(nav)
        .getAllByRole("link")
        .map((a) => a.textContent),
    ).toEqual(["Home", "Attendance", "Results", "Fees", "Notices", "Messages2"]);
    expect(screen.getByTestId("last-updated")).not.toHaveTextContent("more than a day");
  });

  it("warns when the school has been quiet for more than a day", async () => {
    vi.mocked(hubApi.overview).mockResolvedValue({
      ...overview,
      synced_at: "2026-11-10T08:00:00Z",
    });
    open(CHILD);
    expect(await screen.findByTestId("last-updated")).toHaveTextContent(
      "has not sent anything new for more than a day",
    );
  });

  it("shows attendance a month at a time, no further on than this month", async () => {
    open(`${CHILD}/attendance`);
    expect(await screen.findByTestId("month")).toHaveTextContent("November 2026");
    expect(await screen.findByTestId("count-absent")).toHaveTextContent("1");
    expect(screen.getByText("Absent", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByTestId("year-percent")).toHaveTextContent("91.3%");
    expect(screen.getByTestId("month-next")).toBeDisabled();
    await userEvent.click(screen.getByTestId("month-previous"));
    await waitFor(() => expect(hubApi.attendance).toHaveBeenLastCalledWith(REF, "2026-10"));
    expect(await screen.findByText("October 2026")).toBeInTheDocument();
    expect(screen.getByTestId("month-next")).toBeEnabled();
  });

  it("stops going back at the start of the school year", async () => {
    vi.mocked(hubApi.attendance).mockResolvedValue({ ...attendance("2026-04"), days: [] });
    open(`${CHILD}/attendance`);
    expect(await screen.findByText("No attendance was marked in this month.")).toBeInTheDocument();
    expect(screen.getByTestId("month-previous")).toBeDisabled();
  });

  it("shows approved results only: a subject without approved marks is not in the table", async () => {
    open(`${CHILD}/results`);
    const table = await screen.findByTestId("assessment-0");
    expect(within(table).getByRole("row", { name: /English/ })).toHaveTextContent("45 / 50");
    expect(within(table).getByRole("row", { name: /English/ })).toHaveTextContent("90%");
    expect(within(table).queryByText("Mathematics")).not.toBeInTheDocument();
    expect(within(table).getByRole("row", { name: /Total/ })).toHaveTextContent("45 / 50");
    // An assessment not held yet has no block; no position without a complete sheet.
    expect(screen.queryByTestId("assessment-1")).not.toBeInTheDocument();
    expect(screen.queryByTestId("position-0")).not.toBeInTheDocument();
    expect(screen.getByTestId("overall")).toHaveTextContent("90%");
    expect(screen.getByText("Pass")).toBeInTheDocument();
    expect(screen.getByText(/Only results the school has approved are shown/)).toBeInTheDocument();
  });

  it("gives the position in class once the school's sheet is complete", async () => {
    const complete = structuredClone(card);
    complete.totals[0]!.position = 3;
    complete.totals[0]!.students = 30;
    vi.mocked(hubApi.results).mockResolvedValue({ synced_at: NOW, card: complete });
    open(`${CHILD}/results`);
    expect(await screen.findByTestId("position-0")).toHaveTextContent("Position 3 of 30");
  });

  it("says that results come after approval when there are none yet", async () => {
    const none = structuredClone(card);
    none.assessments.forEach((a) => (a.held = false));
    vi.mocked(hubApi.results).mockResolvedValue({ synced_at: NOW, card: none });
    open(`${CHILD}/results`);
    expect(await screen.findByTestId("no-results")).toHaveTextContent(
      "Results appear here once the school has approved them.",
    );
  });

  it("shows what is owed, each voucher and the payments, without a way to pay", async () => {
    open(`${CHILD}/fees`);
    expect(await screen.findByTestId("balance")).toHaveTextContent("PKR 4,500.00");
    expect(screen.getByText(/Fees are paid at the school/)).toBeInTheDocument();
    const may = screen.getByTestId("voucher-V202605-00001");
    expect(within(may).getByText("5,250.00")).toBeInTheDocument();
    expect(within(may).getByText("4,500.00")).toBeInTheDocument();
    expect(screen.getByText("Partly paid")).toBeInTheDocument();
    // A voucher carried forward shows no amount still to pay: it is in the next one.
    const april = screen.getByTestId("voucher-V202604-00001");
    expect(within(april).queryByText("Still to pay")).not.toBeInTheDocument();
    expect(screen.getByText("Carried forward")).toBeInTheDocument();
    expect(screen.getByText(/added to the next voucher/)).toBeInTheDocument();
    expect(screen.getByText("R-00001")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /pay/i })).not.toBeInTheDocument();
  });

  it("shows the school's notices, newest first, marking those for the class", async () => {
    open(`${CHILD}/notices`);
    const first = await screen.findByTestId("notice-n2");
    expect(within(first).getByRole("heading", { name: "Class 5 trip" })).toBeInTheDocument();
    expect(within(first).getByText("For this class")).toBeInTheDocument();
    const second = screen.getByTestId("notice-n1");
    expect(within(second).queryByText("For this class")).not.toBeInTheDocument();
    expect(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("opens the conversation, marks it read once, and sends a message", async () => {
    vi.mocked(hubApi.sendMessage).mockResolvedValue({
      id: "m3",
      sender: "guardian",
      sender_name: "Imran Khan",
      body: "He is better today.",
      sent_at: NOW,
    });
    open(`${CHILD}/messages`);
    expect(await screen.findByTestId("teacher")).toHaveTextContent("Sana Malik");
    expect(screen.getByTestId("message-staff")).toHaveTextContent("Thank you.");
    expect(screen.getByTestId("message-guardian")).toHaveTextContent("You");
    await waitFor(() => expect(hubApi.markRead).toHaveBeenCalledWith(REF));
    // The counts on the bar are asked for again after reading.
    await waitFor(() => expect(hubApi.overview).toHaveBeenCalledTimes(2));

    expect(screen.getByTestId("send")).toBeDisabled();
    await userEvent.type(screen.getByTestId("message-text"), "  He is better today.  ");
    await userEvent.click(screen.getByTestId("send"));
    await waitFor(() =>
      expect(hubApi.sendMessage).toHaveBeenCalledWith(REF, "He is better today."),
    );
    expect(await screen.findByText("He is better today.")).toBeInTheDocument();
    expect(screen.getByTestId("message-text")).toHaveValue("");
    // Honest about the delay.
    expect(screen.getByTestId("sent-note")).toHaveTextContent("it may take a while");
    expect(hubApi.markRead).toHaveBeenCalledTimes(1);
  });

  it("explains why a message cannot be sent", async () => {
    vi.mocked(hubApi.messages).mockResolvedValue({
      ...thread,
      class_teacher: null,
      can_send: false,
      messages: [],
      unread: 0,
    });
    open(`${CHILD}/messages`);
    expect(await screen.findByTestId("cannot-send")).toHaveTextContent("no class teacher yet");
    expect(screen.queryByTestId("message-text")).not.toBeInTheDocument();
    expect(hubApi.markRead).not.toHaveBeenCalled();
  });

  it("keeps the text when the hub refuses a message, and says why", async () => {
    vi.mocked(hubApi.sendMessage).mockRejectedValue(
      new HubError(429, "too_many_messages", "Slow."),
    );
    open(`${CHILD}/messages`);
    await userEvent.type(await screen.findByTestId("message-text"), "One more");
    await userEvent.click(screen.getByTestId("send"));
    expect(await screen.findByRole("alert")).toHaveTextContent("many messages in the last hour");
    expect(screen.getByTestId("message-text")).toHaveValue("One more");
  });

  it("says there is no connection and tries again when asked", async () => {
    vi.mocked(hubApi.fees).mockRejectedValueOnce(new OfflineError()).mockResolvedValueOnce(fees);
    open(`${CHILD}/fees`);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The school network could not be reached",
    );
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("balance")).toBeInTheDocument();
  });

  it("says a child is no longer available when access has ended, with a way back", async () => {
    const gone = new HubError(404, "not_found", "The child was not found.");
    vi.mocked(hubApi.overview).mockRejectedValue(gone);
    vi.mocked(hubApi.results).mockRejectedValue(gone);
    open(`${CHILD}/results`);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "not available to your account any more",
    );
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to my children" })).toHaveAttribute("href", "/");
  });

  it("signs out: the phone forgets the session but keeps the address", async () => {
    open();
    await userEvent.click(await screen.findByTestId("sign-out"));
    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeInTheDocument();
    expect(hubApi.logout).toHaveBeenCalledTimes(1);
    expect(getToken()).toBeNull();
    expect(localStorage.getItem("madrasa.parent.name")).toBeNull();
    expect(screen.getByTestId("hub-address")).toHaveValue(HUB);
  });

  it("adds another school with a second code, asking for the account's password once", async () => {
    vi.mocked(hubApi.checkCode).mockResolvedValue({
      guardian_name: "Imran Khan",
      school_name: "Clifton Grammar",
      children: ["Zara Khan"],
      existing_phone_hint: null,
    });
    vi.mocked(hubApi.activate).mockResolvedValue(session);
    open();
    await userEvent.click(await screen.findByRole("link", { name: "I have another access code" }));
    expect(screen.getByTestId("hub-address")).toHaveValue(HUB);
    await userEvent.type(screen.getByTestId("access-code"), "WXYZ-2345-6789");
    await userEvent.click(screen.getByTestId("check-code"));
    expect(await screen.findByText("Clifton Grammar")).toBeInTheDocument();
    expect(screen.getByTestId("phone")).toHaveValue("+923001111111");
    expect(screen.queryByTestId("password-again")).not.toBeInTheDocument();
    await userEvent.type(screen.getByTestId("password"), "parent-password-1");
    await userEvent.click(screen.getByTestId("activate"));
    await waitFor(() =>
      expect(hubApi.activate).toHaveBeenCalledWith(
        HUB,
        "WXYZ-2345-6789",
        "+923001111111",
        "parent-password-1",
      ),
    );
    expect(await screen.findByRole("heading", { name: "My children" })).toBeInTheDocument();
  });

  it("is in Urdu, right to left, and switches language on the spot", async () => {
    open("/", "ur");
    expect(await screen.findByRole("heading", { name: "میرے بچے" })).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
    expect(document.documentElement).toHaveAttribute("lang", "ur");
    await userEvent.click(screen.getByRole("button", { name: "English" }));
    expect(await screen.findByRole("heading", { name: "My children" })).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("dir", "ltr");
  });

  it("keeps a class name such as 5-A in one piece inside Urdu text", async () => {
    open("/", "ur");
    const link = await screen.findByTestId("child-ali");
    // Wrapped in left-to-right isolates, so the dash does not reorder it.
    expect(link.textContent).toContain("\u20665-A\u2069");
  });

  it("has Urdu names for the sections of a child", async () => {
    open(CHILD, "ur");
    const nav = await screen.findByRole("navigation", { name: "حصے" });
    expect(within(nav).getByRole("link", { name: "حاضری" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: "فیس" })).toBeInTheDocument();
    expect(await screen.findByTestId("tile-fees")).toHaveTextContent("واجب الادا رقم");
  });
});
