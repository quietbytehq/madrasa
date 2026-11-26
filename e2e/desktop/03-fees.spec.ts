import { expect, test } from "@playwright/test";
import { call, tokenFor } from "../support/api";
import { openDesktop } from "../support/desktop";
import { downloadFrom, pdfPages, pdfText } from "../support/pdf";

interface Voucher {
  id: string;
  voucher_no: string;
  student_id: string;
  student_number: string;
  student_name: string;
  total_due: string;
  late_fee: string;
  net_paid: string;
  status: string;
}

/** 8064.00 as the app writes money for this school: "Rs 8,064.00". */
function rupees(amount: string): string {
  const [whole, cents = "00"] = Number(amount).toFixed(2).split(".");
  return `Rs ${whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",")}.${cents}`;
}

async function unpaidVouchers(): Promise<Voucher[]> {
  return call<Voucher[]>("/fees/vouchers?status=unpaid&per_page=200", {
    token: await tokenFor("admin"),
  });
}

test.describe("fees", () => {
  test("the voucher list shows money in the school's currency and filters by status", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { route: "/fees" });
    await expect(page.getByRole("heading", { level: 1, name: "Fees & Finance" })).toBeVisible();
    const rows = page.locator("tbody tr");
    await expect(rows.first()).toContainText(/Rs [\d,]+\.\d{2}/);
    await page.getByLabel("Status").selectOption({ label: "Paid" });
    await expect(rows.first()).toContainText(/Rs [\d,]+\.\d{2}Paid$/);
    await expect(page.locator("tbody")).not.toContainText(/Unpaid|Partial/);
    await context.close();
  });

  test("a voucher prints as a PDF with the student, the number and the currency as real text", async ({
    browser,
  }) => {
    const voucher = (await unpaidVouchers())[0];
    const { page, context } = await openDesktop(browser, {
      route: `/fees/vouchers/${voucher.id}/print`,
    });
    await expect(
      page.getByRole("heading", { name: `Print voucher ${voucher.voucher_no}` }),
    ).toBeVisible();
    // On screen: two copies, each with the amount.
    await expect(page.getByText("School copy")).toBeVisible();
    await expect(page.getByText("Parent copy")).toBeVisible();

    const { download, path } = await downloadFrom(page, () =>
      page.getByRole("button", { name: "Print / Save as PDF" }).click(),
    );
    // Its own name, not "download": the app is allowed to read what the server called it.
    expect(download.suggestedFilename()).toMatch(/\.pdf$/);
    expect(download.suggestedFilename()).toContain(voucher.voucher_no);
    expect(pdfPages(path)).toBeGreaterThanOrEqual(1);
    const text = pdfText(path);
    expect(text).toContain(voucher.voucher_no);
    expect(text).toContain(voucher.student_name);
    expect(text).toContain(voucher.student_number);
    // The currency is written out, never a bare number or a missing glyph.
    expect(text).toContain(rupees(voucher.total_due));
    expect(text).not.toMatch(/�/);
    await context.close();
  });

  test("a payment is recorded from the Collect tab and a receipt is issued", async ({
    browser,
  }) => {
    const voucher = (await unpaidVouchers()).find((v) => Number(v.net_paid) === 0)!;
    const { page, context } = await openDesktop(browser, { route: "/fees" });
    await page.getByRole("tab", { name: "Collect", exact: true }).click();
    await page.getByLabel("Find a student by name or number").fill(voucher.student_number);
    await page
      .getByRole("button", { name: new RegExp(voucher.student_name) })
      .first()
      .click();

    const row = page.locator("tbody tr", { hasText: voucher.voucher_no });
    await expect(row).toBeVisible();
    await row.getByRole("button", { name: "Record payment" }).click();
    const dialog = page.getByRole("dialog", { name: "Record payment" });
    await expect(dialog).toContainText(voucher.voucher_no);
    await dialog.getByLabel("Amount").fill("1000");
    await dialog.getByRole("button", { name: "Record payment" }).click();

    const toast = page.getByText(/^Receipt \S+ issued\.$/);
    await expect(toast).toBeVisible();
    const receiptNo = /^Receipt (\S+) issued\.$/.exec((await toast.textContent()) ?? "")![1];
    await expect(page.getByRole("heading", { name: "Receipts" })).toBeVisible();
    await expect(page.getByRole("listitem").filter({ hasText: receiptNo })).toContainText(
      "Rs 1,000.00",
    );
    await expect(row).toContainText(/Partial/i);

    // The server holds exactly that receipt.
    const after = await call<Voucher>(`/fees/vouchers/${voucher.id}`, {
      token: await tokenFor("admin"),
    });
    expect(Number(after.net_paid)).toBe(1000);
    await context.close();
  });
});
