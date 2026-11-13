import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import { ConnectionProvider, nextLink } from "@/app/connection";
import { OFFLINE_EVENTS, QueuedError, lastQueuedMessage, request, setBaseUrl } from "@/api/client";
import type { OutboxStatus, ProvisionalReceipt, UserSummary, VoucherDetail } from "@/api/types";
import { ConnectionBadge } from "./ConnectionBadge";
import { FormDrafts } from "@/components/drafts/FormDrafts";
import { memoryKeyStore, setKeyStore, listDrafts } from "@/lib/drafts";
import { ProvisionalReceiptPage } from "@/pages/fees/PrintPages";
import { PayDialog } from "@/pages/fees/PayDialog";
import { useToast } from "@/components/ui/Toast";
import { describeChange } from "@/lib/connection";
import { translate } from "@/i18n";

const user: UserSummary = {
  id: "u1",
  school_id: "s1",
  username: "admin",
  full_name: "Ayesha Khan",
  email: null,
  is_active: true,
  roles: ["admin"],
};

type Handler = (url: string, init?: RequestInit) => Response | Promise<Response>;

function json(status: number, body: unknown, headers: Record<string, string> = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", ...headers },
  });
}

function mockFetch(handler: Handler) {
  const fn = vi.fn((input: RequestInfo | URL, init?: RequestInit) =>
    Promise.resolve(handler(String(input), init)),
  );
  vi.stubGlobal("fetch", fn);
  return fn;
}

const offlineStatus: OutboxStatus = {
  link: "offline",
  since: "2026-11-13T08:00:00Z",
  last_contact: "2026-11-13T07:59:00Z",
  waiting: 3,
  needs_sign_in: false,
  items: [
    {
      id: "a",
      method: "POST",
      path: "/api/v1/students",
      queued_at: "2026-11-13T08:01:00Z",
      provisional_no: null,
      attempts: 0,
      last_error: null,
    },
    {
      id: "b",
      method: "POST",
      path: "/api/v1/fees/vouchers/v1/payments",
      queued_at: "2026-11-13T08:02:00Z",
      provisional_no: "P-K7QM-0001",
      attempts: 0,
      last_error: null,
    },
    {
      id: "c",
      method: "PUT",
      path: "/api/v1/attendance/sheet",
      queued_at: "2026-11-13T08:03:00Z",
      provisional_no: null,
      attempts: 1,
      last_error: "refused",
    },
  ],
  failed: [],
  overwritten: [
    {
      id: "o1",
      item_id: "x",
      path: "/api/v1/students/s1",
      at: "2026-11-13T08:05:00Z",
      label: "Ali Khan",
      field: "address",
      yours: "12 New Road",
      kept: "7 Office Street",
    },
  ],
  notices: [],
  sent: [],
};

beforeEach(() => {
  setBaseUrl("http://proxy");
  localStorage.clear();
});

afterEach(() => {
  vi.unstubAllGlobals();
  setBaseUrl(null);
});

function withSession(ui: React.ReactNode, permissions: string[] = ["fees.manage", "fees.read"]) {
  return (
    <FeaturesProvider initial={{ edition: "lite", features: ["fees"], version: "test" }}>
      <SessionProvider initial={{ user, permissions }}>{ui}</SessionProvider>
    </FeaturesProvider>
  );
}

describe("connection", () => {
  it("goes from Connected to Reconnecting to Offline as the server stays silent", () => {
    expect(nextLink(null, true, 0)).toEqual({ link: "connected", failingSince: null });
    const a = nextLink(null, false, 1_000);
    expect(a).toEqual({ link: "reconnecting", failingSince: 1_000 });
    expect(nextLink(a.failingSince, false, 10_000).link).toBe("reconnecting");
    expect(nextLink(a.failingSince, false, 16_000).link).toBe("offline");
    expect(nextLink(a.failingSince, true, 17_000).link).toBe("connected");
  });

  it("shows Offline with the waiting count, and the panel lists what waits and what was overwritten", async () => {
    mockFetch((url) =>
      url.endsWith("/__madrasa/outbox/status") ? json(200, offlineStatus) : json(404, {}),
    );
    renderWithProviders(
      withSession(
        <ConnectionProvider pollMs={60_000}>
          <ConnectionBadge />
        </ConnectionProvider>,
      ),
    );
    const badge = await screen.findByTestId("connection-badge");
    await waitFor(() => expect(within(badge).getByText("Offline")).toBeInTheDocument());
    expect(within(badge).getByTestId("connection-waiting")).toHaveTextContent("3 waiting");
    expect(within(badge).getByTestId("connection-attention")).toHaveTextContent("1");
    await userEvent.click(badge);
    const panel = await screen.findByTestId("connection-panel");
    const waiting = within(panel).getByTestId("waiting-list");
    expect(within(waiting).getByText("New student")).toBeInTheDocument();
    expect(within(waiting).getByText(/Fee payment/)).toBeInTheDocument();
    expect(within(waiting).getByText("(P-K7QM-0001)")).toBeInTheDocument();
    expect(within(waiting).getByText("Attendance")).toBeInTheDocument();
    const over = within(panel).getByTestId("overwritten-list");
    expect(within(over).getByText("12 New Road")).toHaveClass("line-through");
    expect(within(over).getByText("7 Office Street")).toBeInTheDocument();
  });

  it("on a PC running the server itself, watches the API's health", async () => {
    let healthy = true;
    mockFetch((url) => {
      if (url.includes("/__madrasa/")) return json(404, {});
      if (url.endsWith("/api/v1/health")) {
        if (!healthy) throw new TypeError("Failed to fetch");
        return json(200, { status: "ok" });
      }
      return json(404, {});
    });
    renderWithProviders(
      withSession(
        <ConnectionProvider pollMs={60_000}>
          <ConnectionBadge />
        </ConnectionProvider>,
      ),
    );
    const link = await screen.findByTestId("connection-link");
    await waitFor(() => expect(link).toHaveTextContent("Connected"));
    healthy = false;
    act(() => {
      window.dispatchEvent(new CustomEvent(OFFLINE_EVENTS.unreachable));
    });
    await waitFor(() =>
      expect(screen.getByTestId("connection-link")).toHaveTextContent("Reconnecting"),
    );
    expect(screen.queryByTestId("connection-waiting")).toBeNull();
  });

  it("names waiting changes in Urdu too", () => {
    const t = (k: Parameters<typeof translate>[1], p?: Record<string, string | number>) =>
      translate("ur", k, p);
    expect(describeChange(t, "POST", "/api/v1/fees/vouchers/v9/payments")).toBe("فیس کی ادائیگی");
    expect(describeChange(t, "DELETE", "/api/v1/timetable/x")).toBe("تبدیلی: timetable/x");
  });
});

describe("changes kept on this PC", () => {
  it("a 202 from the proxy is a QueuedError worded for people, and toasts show it as information", async () => {
    mockFetch(() =>
      json(202, {
        queued: { id: "q1", queued_at: "2026-11-13T08:00:00Z", waiting: 2, provisional: null },
      }),
    );
    const seen: unknown[] = [];
    const listen = (e: Event) => seen.push((e as CustomEvent).detail);
    window.addEventListener(OFFLINE_EVENTS.queued, listen);
    const err = await request("/students", { method: "POST", body: { full_name: "B" } }).catch(
      (e: unknown) => e,
    );
    window.removeEventListener(OFFLINE_EVENTS.queued, listen);
    expect(err).toBeInstanceOf(QueuedError);
    expect((err as QueuedError).message).toMatch(/^Saved on this PC\..*2 waiting/);
    expect(lastQueuedMessage()).toBe((err as QueuedError).message);
    expect(seen).toHaveLength(1);

    function Shout() {
      const toast = useToast();
      return (
        <button
          type="button"
          onClick={() => toast.push({ kind: "error", title: lastQueuedMessage() })}
        >
          shout
        </button>
      );
    }
    renderWithProviders(<Shout />);
    await userEvent.click(screen.getByText("shout"));
    const toast = await screen.findByText(/Saved on this PC/);
    expect(toast.closest("[role]")?.querySelector(".text-danger")).toBeNull();
  });

  const voucher = {
    id: "v1",
    voucher_no: "V2026-0001",
    billing_month: "2026-04-01",
    student_name: "Ali Khan",
    father_name: "Ahmed Khan",
    student_number: "KHI-2026-0001",
    class_name: "1",
    section_name: "A",
    balance: "2500.00",
    due_date: "2026-04-10",
    late_fee: "0",
    late_fee_waived: false,
    net_monthly: "2500",
    policy: { late_fee_percent: "0" },
    school: { name: "Gulshan", code: "KHI-MC", city: "Karachi", currency: "Rs" },
  } as unknown as VoucherDetail;

  const provisional: ProvisionalReceipt = {
    provisional_no: "P-K7QM-0001",
    voucher_id: "v1",
    voucher,
    amount: "1000",
    method: "cash",
    reference: null,
    note: null,
    paid_on: "2026-11-13",
    taken_at: "2026-11-13T08:00:00Z",
    receipt_no: null,
    receipt_id: null,
  };

  it("a payment taken offline prints a provisional receipt, clearly marked", async () => {
    mockFetch((url) =>
      url.endsWith("/payments")
        ? json(202, {
            queued: { id: "q1", queued_at: "2026-11-13T08:00:00Z", waiting: 1, provisional },
          })
        : url.includes("/__madrasa/outbox/receipts/P-K7QM-0001")
          ? json(200, provisional)
          : json(404, {}),
    );
    renderWithProviders(
      withSession(
        <Routes>
          <Route
            path="/"
            element={<PayDialog voucher={voucher} onClose={() => {}} onPaid={() => {}} />}
          />
          <Route
            path="/fees/receipts/provisional/:number/print"
            element={<ProvisionalReceiptPage />}
          />
        </Routes>,
      ),
    );
    await userEvent.clear(screen.getByLabelText("Amount"));
    await userEvent.type(screen.getByLabelText("Amount"), "1000");
    await userEvent.click(screen.getByRole("button", { name: "Record payment" }));
    const receipt = await screen.findByTestId("provisional-receipt");
    expect(
      within(receipt).getByText(/Provisional receipt — not yet confirmed/),
    ).toBeInTheDocument();
    expect(within(receipt).getByTestId("provisional-number")).toHaveTextContent("P-K7QM-0001");
    expect(within(receipt).getByText("Ali Khan")).toBeInTheDocument();
    expect(within(receipt).getByTestId("provisional-status")).toHaveTextContent(
      "has not received this payment yet",
    );
  });

  it("once the server has it, the provisional receipt shows the real number", async () => {
    mockFetch(() => json(200, { ...provisional, receipt_no: "R2026-000123" }));
    renderWithProviders(
      withSession(
        <Routes>
          <Route
            path="/fees/receipts/provisional/:number/print"
            element={<ProvisionalReceiptPage />}
          />
        </Routes>,
      ),
      { route: "/fees/receipts/provisional/P-K7QM-0001/print" },
    );
    expect(await screen.findByTestId("provisional-status")).toHaveTextContent(
      "real receipt number R2026-000123",
    );
  });
});

describe("form drafts", () => {
  beforeEach(() => setKeyStore(memoryKeyStore()));

  function Form({ onSave }: { onSave?: () => void }) {
    return (
      <form
        onSubmit={(e) => {
          e.preventDefault();
          onSave?.();
        }}
      >
        <h2>New student</h2>
        <label>
          Name
          <input name="full_name" defaultValue="" />
        </label>
        <label>
          Password
          <input name="password" type="password" defaultValue="" />
        </label>
        <label>
          Active
          <input name="active" type="checkbox" defaultChecked={false} />
        </label>
        <button type="submit">Save</button>
      </form>
    );
  }

  it("keeps what was typed, encrypted, offers it back after a restart, never the password", async () => {
    const first = renderWithProviders(
      <>
        <Form />
        <FormDrafts user="u1" />
      </>,
    );
    await userEvent.type(screen.getByLabelText("Name"), "Bilal Ahmed");
    await userEvent.type(screen.getByLabelText("Password"), "secret-pass-1");
    await userEvent.click(screen.getByLabelText("Active"));
    await waitFor(async () => expect(await listDrafts("u1")).toHaveLength(1), { timeout: 3000 });
    // On disk (localStorage): sealed; neither the name nor the password readable.
    const stored = Object.keys(localStorage)
      .filter((k) => k.startsWith("madrasa.draft."))
      .map((k) => localStorage.getItem(k) ?? "");
    expect(stored).toHaveLength(1);
    expect(stored[0]).not.toContain("Bilal");
    const [draft] = await listDrafts("u1");
    expect(JSON.stringify(draft)).not.toContain("secret-pass-1");
    expect(draft!.title).toBe("New student");
    // Another user on this PC does not see it.
    expect(await listDrafts("u2")).toHaveLength(0);

    // The app closes (or crashes) and opens again.
    first.unmount();
    renderWithProviders(
      <>
        <Form />
        <FormDrafts user="u1" />
      </>,
    );
    const offer = await screen.findByTestId("draft-offer");
    expect(offer).toHaveTextContent("Unsaved changes in “New student”");
    await userEvent.click(within(offer).getByTestId("draft-restore"));
    expect(screen.getByLabelText("Name")).toHaveValue("Bilal Ahmed");
    expect(screen.getByLabelText("Active")).toBeChecked();
    expect(screen.getByLabelText("Password")).toHaveValue("");
  });

  it("a saved change removes the draft", async () => {
    renderWithProviders(
      <>
        <Form onSave={() => window.dispatchEvent(new CustomEvent(OFFLINE_EVENTS.saved))} />
        <FormDrafts user="u1" />
      </>,
    );
    await userEvent.type(screen.getByLabelText("Name"), "Sara");
    await waitFor(async () => expect(await listDrafts("u1")).toHaveLength(1), { timeout: 3000 });
    fireEvent.submit(screen.getByRole("button", { name: "Save" }).closest("form")!);
    await waitFor(async () => expect(await listDrafts("u1")).toHaveLength(0), { timeout: 3000 });
  });
});
