import { expect, test } from "@playwright/test";
import { openDesktop } from "../support/desktop";

test.describe("attendance", () => {
  test("a correction on a submitted day is saved, survives a reload and is in the history", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { route: "/attendance" });
    await expect(page.getByRole("heading", { level: 1, name: "Attendance" })).toBeVisible();
    const summary = page.getByTestId("mark-summary");
    await expect(summary).toHaveText(/\d+ present · \d+ absent · \d+ late · \d+ leave/);

    // Give the second student a status they do not have now.
    const row = page.locator('[data-testid^="row-"]').nth(1);
    const student = (await row.getAttribute("data-testid"))!;
    const now = await row.getByRole("radio", { checked: true }).textContent();
    const next = now?.trim() === "Late" ? "Absent" : "Late";
    await row.getByRole("radio", { name: next, exact: true }).click();
    await expect(row.getByRole("radio", { name: next, exact: true })).toBeChecked();
    const counted = await summary.textContent();

    await page.getByRole("button", { name: /^(Save correction|Submit attendance)$/ }).click();
    await expect(page.getByText("Attendance saved.")).toBeVisible();

    await page.reload();
    const again = page.getByTestId(student);
    await expect(again.getByRole("radio", { name: next, exact: true })).toBeChecked();
    await expect(page.getByTestId("mark-summary")).toHaveText(counted!);

    await page.getByRole("button", { name: "History" }).click();
    await expect(page.getByRole("dialog")).toContainText(/Corrected|Submitted the sheet/);
    await context.close();
  });

  test("the monthly register and the absentee list open with the section's students", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { route: "/attendance" });
    await page.getByRole("tab", { name: "Monthly register" }).click();
    await expect(page.locator("tbody tr").first()).toBeVisible();
    await expect(page.locator("thead")).toContainText("1");
    await page.getByRole("tab", { name: "Absentees" }).click();
    await expect(page.getByRole("tabpanel")).toContainText(/absent|Absent|No /);
    await context.close();
  });
});
