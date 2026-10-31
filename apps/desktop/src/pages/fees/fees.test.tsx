import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { renderWithProviders } from "@/test/render";
import { api } from "@/api/endpoints";
import { SessionProvider } from "@/app/session";
import type {
  FeePolicy,
  FeeVoucher,
  FeeVoucherRow,
  Ledger,
  ReceiptDetail,
  UserSummary,
  VoucherDetail,
} from "@/api/types";
import { LedgerView } from "./LedgerView";
import { PayDialog } from "./PayDialog";
import { VouchersTab } from "./tabs/VouchersTab";
import { SetupTab } from "./tabs/SetupTab";

vi.mock("@/api/endpoints", () => ({
  api: {
    classes: vi.fn(async () => [{ id: "c1", school_id: "s", name: "1", level: 1 }]),
    ledger: vi.fn(),
    voucher: vi.fn(),
    vouchers: vi.fn(),
    payVoucher: vi.fn(),
    waiveLateFee: vi.fn(),
    cancelVoucher: vi.fn(),
    reverseReceipt: vi.fn(),
    endConcession: vi.fn(),
    createConcession: vi.fn(),
    generateVouchers: vi.fn(),
    feePolicy: vi.fn(),
    updateFeePolicy: vi.fn(),
    feeHeads: vi.fn(async () => []),
    feeStructure: vi.fn(async () => []),
    academicYears: vi.fn(async () => []),
  },
}));

const user: UserSummary = {
  id: "u1",
  school_id: "s",
  username: "acc",
  full_name: "Bilal Ahmed",
  email: null,
  is_active: true,
  roles: ["accountant"],
};
const ACCOUNTANT = ["fees.read", "fees.manage", "academics.read"];
const ADMIN = [...ACCOUNTANT, "fees.approve"];

const policy: FeePolicy = {
  sibling_discount_percent: "10",
  sibling_rule: "all",
  concession_base: "gross",
  late_fee_percent: "10",
  due_day: 15,
};

const voucher: FeeVoucher = {
  id: "v1",
  student_id: "st1",
  academic_year_id: "y",
  section_id: "sec1",
  voucher_no: "V202605-00001",
  billing_month: "2026-05-01",
  issue_date: "2026-05-01",
  due_date: "2026-05-15",
  gross: "3333.33",
  sibling_discount: "333.33",
  concession_discount: "0",
  net_monthly: "3000.00",
  arrears: "0",
  total_due: "3000.00",
  late_fee: "300.00",
  late_fee_waived: false,
  waiver_reason: null,
  waived_by: null,
  net_paid: "1000.00",
  status: "partial",
};

const ledger: Ledger = {
  student_id: "st1",
  totals: {
    billed: "3333.33",
    discounts: "333.33",
    fines: "300.00",
    paid: "1000.00",
    balance: "2300.00",
  },
  vouchers: [{ ...voucher, balance: "2300.00" }],
  receipts: [
    {
      id: "r1",
      voucher_id: "v1",
      student_id: "st1",
      receipt_no: "R2026-000001",
      paid_on: "2026-05-10",
      amount: "1000.00",
      method: "kuickpay",
      reference: "KP-123",
      note: null,
      reverses_id: null,
      reversal_reason: null,
      received_by: "u1",
      created_at: "2026-05-10T09:00:00Z",
    },
  ],
  concessions: [],
  payable: ["v1"],
};

const detail: VoucherDetail = {
  ...voucher,
  balance: "2300.00",
  lines: [],
  receipts: [],
  student_name: "Ali Khan",
  student_number: "KHI-MC-2026-00001",
  father_name: "Ahmed Khan",
  class_name: "1",
  section_name: "A",
  guardian_phone: null,
  school: { name: "Gulshan High", code: "GHS", city: "Karachi", currency: "PKR" },
  policy,
};

function renderLedger(permissions = ADMIN) {
  return renderWithProviders(
    <SessionProvider initial={{ user, permissions }}>
      <Routes>
        <Route path="/students/:id" element={<LedgerView studentId="st1" />} />
      </Routes>
    </SessionProvider>,
    { route: "/students/st1" },
  );
}

beforeEach(() => {
  vi.mocked(api.ledger).mockResolvedValue(ledger);
  vi.mocked(api.voucher).mockResolvedValue(detail);
  vi.mocked(api.waiveLateFee).mockResolvedValue({ ...detail, late_fee_waived: true });
  vi.mocked(api.reverseReceipt).mockClear();
  vi.mocked(api.payVoucher).mockClear();
  vi.mocked(api.waiveLateFee).mockClear();
});

describe("LedgerView", () => {
  it("shows totals, vouchers and receipts", async () => {
    renderLedger();
    expect(await screen.findByText("V202605-00001")).toBeInTheDocument();
    // Balance appears in the totals card and in the voucher row.
    expect(screen.getAllByText("Rs 2,300.00")).toHaveLength(2);
    expect(screen.getByText("R2026-000001")).toBeInTheDocument();
    expect(screen.getByText(/Kuickpay · KP-123/)).toBeInTheDocument();
    expect(screen.getByText("Partial")).toBeInTheDocument();
  });

  it("lets an accountant pay but not waive, cancel or reverse", async () => {
    renderLedger(ACCOUNTANT);
    await screen.findByText("V202605-00001");
    expect(screen.getByRole("button", { name: "Record payment" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Waive late fee" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Reverse receipt" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add concession" })).not.toBeInTheDocument();
  });

  it("waives a late fee with a reason when the user can approve", async () => {
    renderLedger();
    await screen.findByText("V202605-00001");
    const loads = vi.mocked(api.ledger).mock.calls.length;
    await userEvent.click(screen.getByRole("button", { name: "Waive late fee" }));
    const dialog = screen.getByRole("dialog");
    const submit = within(dialog).getByRole("button", { name: "Waive late fee" });
    expect(submit).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Reason for waiver"), "Family hardship");
    await userEvent.click(submit);
    await waitFor(() => expect(api.waiveLateFee).toHaveBeenCalledWith("v1", "Family hardship"));
    await waitFor(() => expect(vi.mocked(api.ledger).mock.calls.length).toBeGreaterThan(loads));
  });

  it("reverses a receipt with a reason", async () => {
    vi.mocked(api.reverseReceipt).mockResolvedValue({} as ReceiptDetail);
    renderLedger();
    await screen.findByText("R2026-000001");
    await userEvent.click(screen.getByRole("button", { name: "Reverse receipt" }));
    const dialog = screen.getByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Reason for reversal"), "Wrong student");
    await userEvent.click(within(dialog).getByRole("button", { name: "Reverse receipt" }));
    await waitFor(() => expect(api.reverseReceipt).toHaveBeenCalledWith("r1", "Wrong student"));
  });

  it("opens the pay dialog from the ledger and reports the receipt", async () => {
    vi.mocked(api.payVoucher).mockResolvedValue({
      receipt: { ...ledger.receipts[0]!, receipt_no: "R2026-000002" } as ReceiptDetail,
      voucher: { ...detail, net_paid: "3300.00", balance: "0", status: "paid" },
    });
    renderLedger();
    await screen.findByText("V202605-00001");
    await userEvent.click(screen.getByRole("button", { name: "Record payment" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Amount")).toHaveValue("2300.00");
    await userEvent.click(within(dialog).getByRole("button", { name: "Record payment" }));
    await waitFor(() =>
      expect(api.payVoucher).toHaveBeenCalledWith(
        "v1",
        expect.objectContaining({ amount: "2300.00", method: "cash" }),
      ),
    );
    expect(await screen.findByText("Receipt R2026-000002 issued.")).toBeInTheDocument();
  });
});

describe("PayDialog", () => {
  const onPaid = vi.fn();
  function renderPay(v: VoucherDetail = detail) {
    return renderWithProviders(<PayDialog voucher={v} onClose={() => {}} onPaid={onPaid} />);
  }

  it("rejects a malformed amount and a missing reference for non-cash methods", async () => {
    renderPay();
    const amount = screen.getByLabelText("Amount");
    await userEvent.clear(amount);
    await userEvent.type(amount, "12.345");
    await userEvent.click(screen.getByRole("button", { name: "Record payment" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Amount");
    expect(api.payVoucher).not.toHaveBeenCalled();

    await userEvent.clear(amount);
    await userEvent.type(amount, "1,500");
    await userEvent.selectOptions(screen.getByLabelText("Method"), "bank");
    await userEvent.click(screen.getByRole("button", { name: "Record payment" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Reference no.");
    expect(api.payVoucher).not.toHaveBeenCalled();

    await userEvent.type(screen.getByLabelText("Reference no."), "TRX-9");
    await userEvent.click(screen.getByRole("button", { name: "Record payment" }));
    await waitFor(() =>
      expect(api.payVoucher).toHaveBeenCalledWith(
        "v1",
        expect.objectContaining({ amount: "1500.00", method: "bank", reference: "TRX-9" }),
      ),
    );
  });

  it("warns that a late fee will be added when paying after the due date", async () => {
    const d = new Date().toISOString().slice(0, 10);
    renderPay({ ...detail, late_fee: "0", due_date: "2000-01-01", net_monthly: "2500.00" });
    expect(screen.getByRole("status")).toHaveTextContent("250.00");
    expect(screen.getByLabelText("Payment date")).toHaveValue(d);
  });

  it("does not warn when the late fee is already assessed or waived", () => {
    renderPay({ ...detail, late_fee: "0", late_fee_waived: true, due_date: "2000-01-01" });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});

describe("VouchersTab", () => {
  const rows: FeeVoucherRow[] = [
    {
      id: "v1",
      voucher_no: "V202605-00001",
      billing_month: "2026-05-01",
      due_date: "2026-05-15",
      student_id: "st1",
      student_number: "KHI-MC-2026-00001",
      student_name: "Ali Khan",
      class_name: "1",
      section_name: "A",
      gross: "3333.33",
      sibling_discount: "333.33",
      concession_discount: "0",
      arrears: "0",
      total_due: "3000.00",
      late_fee: "300.00",
      late_fee_waived: false,
      net_paid: "1000.00",
      status: "partial",
    },
    {
      id: "v2",
      voucher_no: "V202605-00002",
      billing_month: "2026-05-01",
      due_date: "2026-05-15",
      student_id: "st2",
      student_number: "KHI-MC-2026-00002",
      student_name: "Sara Khan",
      class_name: "1",
      section_name: "A",
      gross: "3333.33",
      sibling_discount: "333.33",
      concession_discount: "0",
      arrears: "0",
      total_due: "3000.00",
      late_fee: "0",
      late_fee_waived: false,
      net_paid: "3000.00",
      status: "paid",
    },
  ];

  function renderTab(permissions = ACCOUNTANT) {
    return renderWithProviders(
      <SessionProvider initial={{ user, permissions }}>
        <VouchersTab />
      </SessionProvider>,
    );
  }

  it("lists vouchers with totals and generates for the month", async () => {
    vi.mocked(api.vouchers).mockResolvedValue(rows);
    vi.mocked(api.generateVouchers).mockResolvedValue({
      created: 40,
      skipped_existing: 2,
      skipped_no_structure: 0,
      voucher_ids: [],
    });
    renderTab();
    expect(await screen.findByText("Ali Khan")).toBeInTheDocument();
    expect(screen.getByText("2 results", { exact: false })).toBeInTheDocument();
    // Footer: 3000 + 300 + 3000 due, 1000 + 3000 paid.
    expect(screen.getByText("Rs 6,300.00")).toBeInTheDocument();
    expect(screen.getByText("Rs 4,000.00")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Generate vouchers" }));
    const dialog = screen.getByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Generate vouchers" }));
    await waitFor(() =>
      expect(api.generateVouchers).toHaveBeenCalledWith(
        expect.objectContaining({ month: expect.stringMatching(/^\d{4}-\d{2}-01$/) }),
      ),
    );
    expect(await screen.findByText(/40 vouchers created/)).toBeInTheDocument();
  });

  it("hides generation from read-only users", async () => {
    vi.mocked(api.vouchers).mockResolvedValue([]);
    renderTab(["fees.read"]);
    expect(await screen.findByText("No vouchers for this month yet.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Generate vouchers" })).not.toBeInTheDocument();
  });
});

describe("SetupTab policy", () => {
  it("saves an edited policy", async () => {
    vi.mocked(api.feePolicy).mockResolvedValue(policy);
    vi.mocked(api.updateFeePolicy).mockImplementation(async (p) => p);
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <SetupTab />
      </SessionProvider>,
    );
    const percent = await screen.findByLabelText("Sibling discount (% of monthly fee)");
    await userEvent.clear(percent);
    await userEvent.type(percent, "12.5");
    await userEvent.selectOptions(
      screen.getByLabelText("Who gets the sibling discount"),
      "second_child_onwards",
    );
    const dueDay = screen.getByLabelText("Due day of the month");
    await userEvent.clear(dueDay);
    await userEvent.type(dueDay, "10");
    await userEvent.click(screen.getAllByRole("button", { name: "Save" })[0]!);
    await waitFor(() =>
      expect(api.updateFeePolicy).toHaveBeenCalledWith({
        sibling_discount_percent: "12.5",
        sibling_rule: "second_child_onwards",
        concession_base: "gross",
        late_fee_percent: "10",
        due_day: 10,
      }),
    );
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
  });

  it("explains the concession base with a worked example and saves the choice", async () => {
    vi.mocked(api.feePolicy).mockResolvedValue(policy);
    vi.mocked(api.updateFeePolicy).mockImplementation(async (p) => p);
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <SetupTab />
      </SessionProvider>,
    );
    const select = await screen.findByLabelText("Percentage concessions are calculated on");
    expect(select).toHaveValue("gross");
    // Worked example follows the sibling percent: 10% of 3,000 = 300; 25% of 3,000 vs 2,700.
    expect(
      screen.getByText(
        "Example: monthly fee Rs 3,000.00, sibling discount 10% = Rs 300.00, scholarship 25%.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("On gross fee: 25% of Rs 3,000.00 = Rs 750.00; student pays Rs 1,950.00."),
    ).toHaveClass("font-semibold");
    expect(
      screen.getByText(
        "After sibling discount: 25% of Rs 2,700.00 = Rs 675.00; student pays Rs 2,025.00.",
      ),
    ).not.toHaveClass("font-semibold");

    await userEvent.selectOptions(select, "after_sibling_discount");
    expect(
      screen.getByText(
        "After sibling discount: 25% of Rs 2,700.00 = Rs 675.00; student pays Rs 2,025.00.",
      ),
    ).toHaveClass("font-semibold");
    await userEvent.click(screen.getAllByRole("button", { name: "Save" })[0]!);
    await waitFor(() =>
      expect(api.updateFeePolicy).toHaveBeenCalledWith(
        expect.objectContaining({ concession_base: "after_sibling_discount" }),
      ),
    );
  });

  it("renders the concession base explanation in Urdu", async () => {
    vi.mocked(api.feePolicy).mockResolvedValue(policy);
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ADMIN }}>
        <SetupTab />
      </SessionProvider>,
      { locale: "ur" },
    );
    expect(await screen.findByLabelText("فیصدی رعایت کس رقم پر لگے")).toHaveValue("gross");
    expect(screen.getByText(/مجموعی فیس پر: Rs 3,000.00 کا 25%/)).toBeInTheDocument();
  });

  it("is read-only without the approve permission", async () => {
    vi.mocked(api.feePolicy).mockResolvedValue(policy);
    renderWithProviders(
      <SessionProvider initial={{ user, permissions: ACCOUNTANT }}>
        <SetupTab />
      </SessionProvider>,
    );
    expect(await screen.findByLabelText("Sibling discount (% of monthly fee)")).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });
});
