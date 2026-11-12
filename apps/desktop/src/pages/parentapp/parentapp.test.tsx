import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import { Sidebar } from "@/layout/Sidebar";
import type {
  GuardianAccessRow,
  GuardianView,
  ParentConversation,
  ParentConversationRow,
  ParentNotice,
  UserSummary,
} from "@/api/types";
import { ParentAppPage } from "./ParentAppPage";
import { noticeState } from "./NoticesTab";

vi.mock("@/api/endpoints", () => ({
  api: {
    parentAppStatus: vi.fn(),
    parentAccess: vi.fn(),
    parentAccessCode: vi.fn(),
    parentAccessEnd: vi.fn(),
    parentNotices: vi.fn(),
    createParentNotice: vi.fn(),
    updateParentNotice: vi.fn(),
    publishParentNotice: vi.fn(),
    withdrawParentNotice: vi.fn(),
    deleteParentNotice: vi.fn(),
    parentConversations: vi.fn(),
    parentConversation: vi.fn(),
    sendParentMessage: vi.fn(),
    markParentConversationRead: vi.fn(),
    searchGuardians: vi.fn(),
    classes: vi.fn(),
  },
}));

const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "sana",
  full_name: "Sana Malik",
  email: null,
  is_active: true,
  roles: ["teacher"],
};

const PRINCIPAL = ["messages.reply", "messages.view_all", "notices.manage", "parents.manage"];
const FEATURES = ["dashboard", "students", "nexus_link"] as const;

function renderPage(permissions: string[], route = "/parent-app", locale: "en" | "ur" = "en") {
  return renderWithProviders(
    <FeaturesProvider initial={{ edition: "lite", features: [...FEATURES] }}>
      <SessionProvider initial={{ user, permissions }}>
        <ParentAppPage />
      </SessionProvider>
    </FeaturesProvider>,
    { route, locale },
  );
}

const row: ParentConversationRow = {
  student_id: "st1",
  student_name: "Ali Khan",
  guardian_id: "g1",
  guardian_name: "Imran Khan",
  section_id: "sec1",
  class_name: "5",
  section_name: "A",
  messages: 1,
  last_at: "2026-11-12T08:00:00Z",
  last_sender: "guardian",
  last_preview: "Ali has a fever today.",
  unread: 1,
  can_reply: true,
};

const thread: ParentConversation = {
  student_id: "st1",
  student_name: "Ali Khan",
  guardian_id: "g1",
  guardian_name: "Imran Khan",
  class_name: "5",
  section_name: "A",
  teacher_name: "Sana Malik",
  can_reply: true,
  unread: 1,
  messages: [
    {
      id: "m1",
      student_id: "st1",
      guardian_id: "g1",
      sender: "guardian",
      sender_user_id: null,
      sender_name: "Imran Khan",
      body: "Ali has a fever today.",
      sent_at: "2026-11-12T08:00:00Z",
    },
  ],
};

const notice: ParentNotice = {
  id: "n1",
  school_id: "s",
  title: "Sports day",
  body: "Friday. Bring water.",
  class_ids: [],
  published_at: "2026-11-10T08:00:00Z",
  expires_on: null,
  created_by: "u1",
  created_at: "2026-11-10T08:00:00Z",
  updated_at: "2026-11-10T08:00:00Z",
};

const access: GuardianAccessRow = {
  guardian_id: "g1",
  state: "active",
  phone_hint: "•••• 1111",
  since: "2026-11-01T08:00:00Z",
  last_login_at: "2026-11-11T08:00:00Z",
  code_expires_at: null,
  guardian_name: "Imran Khan",
  children: ["Ali Khan"],
};

const guardian = (id: string, name: string, children: string[]): GuardianView => ({
  id,
  school_id: "s",
  full_name: name,
  phone: "0300-2222222",
  email: null,
  cnic: null,
  occupation: "",
  address: "",
  students: children.map((full_name, i) => ({
    id: `${id}-s${i}`,
    student_number: "2026-0001",
    full_name,
    father_name: "F",
    gender: "male" as const,
    date_of_birth: "2015-01-01",
    status: "active" as const,
    admission_date: "2026-04-01",
    class_id: "c5",
    class_name: "5",
    section_id: "sec1",
    section_name: "A",
    roll_no: 1,
    has_photo: false,
  })),
});

describe("Parent app area", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.parentAppStatus).mockResolvedValue({
      joined: true,
      hub_name: "Gulshan Education Network",
      unread: 1,
    });
    vi.mocked(api.parentConversations).mockResolvedValue([row]);
    vi.mocked(api.parentConversation).mockResolvedValue(thread);
    vi.mocked(api.markParentConversationRead).mockResolvedValue(undefined);
    vi.mocked(api.parentNotices).mockResolvedValue([notice]);
    vi.mocked(api.parentAccess).mockResolvedValue([access]);
    vi.mocked(api.classes).mockResolvedValue([
      { id: "c5", school_id: "s", name: "5", level: 5 },
      { id: "c6", school_id: "s", name: "6", level: 6 },
    ]);
  });

  it("explains that a school network is needed, and shows nothing else, when not joined", async () => {
    vi.mocked(api.parentAppStatus).mockResolvedValue({ joined: false, hub_name: null, unread: 0 });
    renderPage(PRINCIPAL);
    expect(
      await screen.findByText("The parent app works through a school network"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    expect(api.parentConversations).not.toHaveBeenCalled();
    expect(api.parentAccess).not.toHaveBeenCalled();
  });

  it("gives each person only the tabs their permissions allow", async () => {
    const { unmount } = renderPage(["messages.reply"]);
    expect(await screen.findByRole("tab", { name: /Messages/ })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Notices" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Access" })).not.toBeInTheDocument();
    unmount();

    renderPage(["notices.manage", "parents.manage"], "/parent-app?tab=access");
    expect(await screen.findByRole("tab", { name: "Access" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.queryByRole("tab", { name: /Messages/ })).not.toBeInTheDocument();
    expect(api.parentConversations).toHaveBeenCalledTimes(1); // from the first render only
  });

  it("lets the class teacher read a conversation, marks it read and sends an answer", async () => {
    const answered: ParentConversation = {
      ...thread,
      unread: 0,
      messages: [
        ...thread.messages,
        {
          id: "m2",
          student_id: "st1",
          guardian_id: "g1",
          sender: "staff",
          sender_user_id: "u1",
          sender_name: "Sana Malik",
          body: "Thank you.",
          sent_at: "2026-11-12T09:00:00Z",
        },
      ],
    };
    vi.mocked(api.sendParentMessage).mockResolvedValue(answered.messages[1]!);
    renderPage(["messages.reply"]);
    expect(await screen.findByTestId("tab-unread")).toHaveTextContent("1");
    await userEvent.click(await screen.findByTestId("conversation-st1"));
    expect(await screen.findByTestId("message-guardian")).toHaveTextContent(
      "Ali has a fever today.",
    );
    expect(screen.getByText(/Class teacher: Sana Malik/)).toBeInTheDocument();
    await waitFor(() => expect(api.markParentConversationRead).toHaveBeenCalledWith("st1", "g1"));

    const send = screen.getByTestId("send-reply");
    expect(send).toBeDisabled();
    vi.mocked(api.parentConversation).mockResolvedValue(answered);
    await userEvent.type(screen.getByTestId("reply-text"), "  Thank you.  ");
    await userEvent.click(send);
    await waitFor(() =>
      expect(api.sendParentMessage).toHaveBeenCalledWith("st1", "g1", "Thank you."),
    );
    expect(await screen.findByTestId("message-staff")).toHaveTextContent("Thank you.");
    expect(screen.getByTestId("reply-text")).toHaveValue("");
    expect(api.markParentConversationRead).toHaveBeenCalledTimes(1);
  });

  it("lets the principal read a conversation without answering or marking it read", async () => {
    vi.mocked(api.parentConversations).mockResolvedValue([{ ...row, can_reply: false }]);
    vi.mocked(api.parentConversation).mockResolvedValue({ ...thread, can_reply: false });
    renderPage(["messages.view_all"]);
    expect(await screen.findByText(/Only the class teacher answers/)).toBeInTheDocument();
    await userEvent.click(await screen.findByTestId("conversation-st1"));
    expect(await screen.findByTestId("read-only")).toBeInTheDocument();
    expect(screen.queryByTestId("reply-text")).not.toBeInTheDocument();
    expect(api.markParentConversationRead).not.toHaveBeenCalled();
  });

  it("says so when there are no messages yet", async () => {
    vi.mocked(api.parentConversations).mockResolvedValue([]);
    renderPage(["messages.reply"]);
    expect(await screen.findByText("No messages yet.")).toBeInTheDocument();
  });

  it("writes a notice for chosen classes and publishes it", async () => {
    vi.mocked(api.createParentNotice).mockResolvedValue({ ...notice, id: "n2" });
    renderPage(PRINCIPAL, "/parent-app?tab=notices");
    const table = await screen.findByRole("table");
    expect(within(table).getByText("Sports day")).toBeInTheDocument();
    expect(within(table).getByText("Whole school")).toBeInTheDocument();
    expect(within(table).getByText("Published")).toBeInTheDocument();

    await userEvent.click(screen.getByTestId("new-notice"));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByTestId("notice-title"), "Class 5 trip");
    await userEvent.type(within(dialog).getByTestId("notice-body"), "Permission slips by Monday.");
    // Chosen classes with none ticked is refused before anything is sent.
    await userEvent.click(within(dialog).getByTestId("audience-classes"));
    await userEvent.click(within(dialog).getByTestId("publish-notice"));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Choose at least one class");
    expect(api.createParentNotice).not.toHaveBeenCalled();
    await userEvent.click(within(dialog).getByLabelText("5"));
    await userEvent.click(within(dialog).getByTestId("publish-notice"));
    await waitFor(() =>
      expect(api.createParentNotice).toHaveBeenCalledWith({
        title: "Class 5 trip",
        body: "Permission slips by Monday.",
        class_ids: ["c5"],
        expires_on: null,
        publish: true,
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.parentNotices).toHaveBeenCalledTimes(2);
  });

  it("withdraws a published notice", async () => {
    vi.mocked(api.withdrawParentNotice).mockResolvedValue({ ...notice, published_at: null });
    renderPage(PRINCIPAL, "/parent-app?tab=notices");
    await userEvent.click(await screen.findByText("Sports day"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByTestId("notice-title")).toHaveValue("Sports day");
    expect(within(dialog).queryByTestId("publish-notice")).not.toBeInTheDocument();
    await userEvent.click(within(dialog).getByTestId("withdraw-notice"));
    await waitFor(() => expect(api.withdrawParentNotice).toHaveBeenCalledWith("n1"));
  });

  it("knows a draft from a published notice and one past its last day", () => {
    expect(noticeState({ ...notice, published_at: null })).toBe("draft");
    expect(noticeState(notice, "2026-11-12")).toBe("published");
    expect(noticeState({ ...notice, expires_on: "2026-11-12" }, "2026-11-12")).toBe("published");
    expect(noticeState({ ...notice, expires_on: "2026-11-11" }, "2026-11-12")).toBe("expired");
  });

  it("gives a guardian a code, shown once on a printable slip", async () => {
    vi.mocked(api.searchGuardians).mockResolvedValue([
      guardian("g2", "Nadia Noor", ["Sara Noor"]),
      guardian("g3", "No Child", []),
    ]);
    vi.mocked(api.parentAccessCode).mockResolvedValue({
      code: "ABCD-EFGH-JKMN",
      expires_at: "2026-11-26T08:00:00Z",
      guardian_name: "Nadia Noor",
      hub_url: "https://hub.example.org",
      hub_name: "Gulshan Education Network",
    });
    renderPage(PRINCIPAL, "/parent-app?tab=access");
    const table = await screen.findByRole("table");
    expect(within(table).getByText("Imran Khan")).toBeInTheDocument();
    expect(within(table).getByText("Active")).toBeInTheDocument();
    expect(within(table).getByText("•••• 1111")).toBeInTheDocument();

    await userEvent.click(screen.getByTestId("give-access"));
    await userEvent.type(await screen.findByTestId("guardian-search"), "No");
    expect(await screen.findByText("Nadia Noor")).toBeInTheDocument();
    expect(screen.getByTestId("choose-g3")).toBeDisabled(); // a guardian without students
    await userEvent.click(screen.getByTestId("choose-g2"));
    await waitFor(() => expect(api.parentAccessCode).toHaveBeenCalledWith("g2"));

    const slip = await screen.findByTestId("access-slip");
    expect(within(slip).getByTestId("access-code")).toHaveTextContent("ABCD-EFGH-JKMN");
    expect(within(slip).getByText("https://hub.example.org")).toBeInTheDocument();
    expect(within(slip).getByText("Nadia Noor")).toBeInTheDocument();
    // The copy that prints sits straight under the page, and says the same.
    const printed = screen.getByTestId("access-slip-print");
    expect(printed).toHaveClass("print-area");
    expect(printed.parentElement).toBe(document.body);
    expect(within(printed).getByTestId("access-code")).toHaveTextContent("ABCD-EFGH-JKMN");
    expect(screen.getByText(/shown once and works once/)).toBeInTheDocument();
    // Closing the slip forgets the code.
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByText("ABCD-EFGH-JKMN")).not.toBeInTheDocument());
  });

  it("ends a guardian's access after asking", async () => {
    vi.mocked(api.parentAccessEnd).mockResolvedValue({ ended: true });
    renderPage(PRINCIPAL, "/parent-app?tab=access");
    await userEvent.click(await screen.findByTestId("end-access-g1"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("End Imran Khan's access?")).toBeInTheDocument();
    expect(api.parentAccessEnd).not.toHaveBeenCalled();
    await userEvent.click(within(dialog).getByTestId("confirm-end-access"));
    await waitFor(() => expect(api.parentAccessEnd).toHaveBeenCalledWith("g1"));
    expect(api.parentAccess).toHaveBeenCalledTimes(2);
  });

  it("is in Urdu, right to left names included", async () => {
    renderPage(PRINCIPAL, "/parent-app?tab=notices", "ur");
    expect(await screen.findByRole("tab", { name: /پیغامات/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "اعلانات" })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByText("پورا اسکول")).toBeInTheDocument();
    expect(screen.getByTestId("new-notice")).toHaveTextContent("نیا اعلان");
  });
});

describe("Sidebar: the Parent app entry", () => {
  function renderSidebar(permissions: string[], features: readonly string[] = FEATURES) {
    return renderWithProviders(
      <FeaturesProvider
        initial={{ edition: "lite", features: [...features] as never, version: "test" }}
      >
        <SessionProvider initial={{ user, permissions }}>
          <Sidebar collapsed={false} onToggle={() => {}} />
        </SessionProvider>
      </FeaturesProvider>,
    );
  }
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("appears with the unread count once the school is joined", async () => {
    vi.mocked(api.parentAppStatus).mockResolvedValue({ joined: true, hub_name: "Hub", unread: 3 });
    renderSidebar(["messages.reply"]);
    const link = await screen.findByRole("link", { name: /Parent app/ });
    expect(link).toHaveAttribute("href", "/parent-app");
    expect(within(link).getByTestId("nav-unread")).toHaveTextContent("3");
  });

  it("stays hidden for a school that has not joined", async () => {
    vi.mocked(api.parentAppStatus).mockResolvedValue({ joined: false, hub_name: null, unread: 0 });
    renderSidebar(PRINCIPAL);
    await waitFor(() => expect(api.parentAppStatus).toHaveBeenCalled());
    expect(screen.queryByRole("link", { name: /Parent app/ })).not.toBeInTheDocument();
  });

  it("is not even asked about for people with nothing to do there", () => {
    renderSidebar(["students.read"]);
    expect(api.parentAppStatus).not.toHaveBeenCalled();
    expect(screen.queryByRole("link", { name: /Parent app/ })).not.toBeInTheDocument();
  });
});
