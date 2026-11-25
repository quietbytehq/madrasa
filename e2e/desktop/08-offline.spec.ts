import { expect, test } from "@playwright/test";
import { call, tokenFor } from "../support/api";
import { openDesktop } from "../support/desktop";
import { SCHOOL_PID, URLS } from "../support/env";

interface Voucher {
  id: string;
  voucher_no: string;
  student_number: string;
  student_name: string;
  net_paid: string;
}

/** The school server stops answering, as when its cable is pulled; it is not shut down. */
const freeze = () => process.kill(SCHOOL_PID, "SIGSTOP");
const thaw = () => process.kill(SCHOOL_PID, "SIGCONT");

test.describe("connection badge and offline work", () => {
  test.skip(!SCHOOL_PID, "needs the school server's process id (run through scripts/e2e.sh)");
  test.afterEach(() => thaw());

  test("on the server PC the badge says Connected and explains itself", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { route: "/" });
    await expect(page.getByTestId("connection-link")).toHaveText("Connected");
    await page.getByTestId("connection-badge").click();
    const panel = page.getByTestId("connection-panel");
    await expect(panel).toContainText("Madrasa is running on this PC.");
    await context.close();
  });

  test("a client PC goes Reconnecting, then Offline, queues a payment, and sends it when the server is back", async ({
    browser,
  }) => {
    test.setTimeout(180_000);
    const token = await tokenFor("admin");
    const vouchers = await call<Voucher[]>("/fees/vouchers?status=unpaid&per_page=200", { token });
    const voucher = vouchers.filter((v) => Number(v.net_paid) === 0).at(-1)!;

    const { page, context } = await openDesktop(browser, { api: URLS.client, route: "/fees" });
    const link = page.getByTestId("connection-link");
    await expect(link).toHaveText("Connected");
    await expect(page.getByTestId("connection-waiting")).toHaveCount(0);

    // The cashier has the student's ledger open with the payment typed in…
    await page.getByRole("tab", { name: "Collect", exact: true }).click();
    await page.getByLabel("Find a student by name or number").fill(voucher.student_number);
    await page
      .getByRole("button", { name: new RegExp(voucher.student_name) })
      .first()
      .click();
    const row = page.locator("tbody tr", { hasText: voucher.voucher_no });
    await row.getByRole("button", { name: "Record payment" }).click();
    const dialog = page.getByRole("dialog", { name: "Record payment" });
    await dialog.getByLabel("Amount").fill("500");

    // …when the server stops answering.
    freeze();
    await expect(link).toHaveText("Reconnecting…", { timeout: 30_000 });
    await expect(link).toHaveText("Offline", { timeout: 60_000 });

    await dialog.getByRole("button", { name: "Record payment" }).click();
    // The cashier gets a provisional receipt to hand over, marked as not yet confirmed.
    const heading = page.getByRole("heading", { level: 1, name: /^Provisional receipt P-/ });
    await expect(heading).toBeVisible({ timeout: 20_000 });
    const provisional = /(P-\S+)$/.exec((await heading.textContent()) ?? "")![1];
    await expect(
      page.getByText("Provisional receipt — not yet confirmed by the school server"),
    ).toBeVisible();
    const receipt = page.getByTestId("provisional-receipt");
    await expect(receipt).toContainText(voucher.voucher_no);
    await expect(receipt).toContainText("500.00");
    await expect(page.getByTestId("connection-waiting")).toContainText("1 waiting");

    // The panel lists what is waiting, in words.
    await page.getByTestId("connection-badge").click();
    const panel = page.getByTestId("connection-panel");
    await expect(panel).toContainText("Changes waiting to be sent: 1");
    await expect(panel).toContainText("Fee payment");
    await page.keyboard.press("Escape");

    // The server comes back: the payment arrives once, with a real receipt number.
    thaw();
    await expect(link).toHaveText("Connected", { timeout: 60_000 });
    await expect(page.getByTestId("connection-waiting")).toHaveCount(0, { timeout: 60_000 });
    const after = await call<Voucher>(`/fees/vouchers/${voucher.id}`, { token });
    expect(Number(after.net_paid)).toBe(500);
    await page.getByTestId("connection-badge").click();
    await expect(page.getByTestId("connection-panel")).toContainText(
      new RegExp(`${provisional} is now R\\S+`),
    );
    await context.close();
  });
});
