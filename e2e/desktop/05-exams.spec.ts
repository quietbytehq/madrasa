import { expect, test } from "@playwright/test";
import { openDesktop } from "../support/desktop";
import { downloadFrom, pdfPages, pdfText } from "../support/pdf";

test.describe("exams", () => {
  test("an approved marks sheet is reopened, a mark is changed, and it is approved again", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { route: "/exams" });
    await expect(page.getByRole("heading", { level: 1, name: "Examinations" })).toBeVisible();
    const subject = page
      .locator("tbody tr")
      .filter({ hasText: /Approved/i })
      .first();
    await expect(subject).toBeVisible();
    await subject.getByRole("button", { name: "Open" }).click();

    // Approved: the marks are shown and cannot be typed over.
    const first = page
      .locator("tbody tr")
      .first()
      .getByRole("spinbutton")
      .or(page.locator("tbody tr").first().getByRole("textbox"))
      .first();
    await expect(first).toBeDisabled();
    await page.getByRole("button", { name: "Reopen" }).click();
    await expect(first).toBeEnabled();

    // More than the maximum is refused in words.
    await first.fill("999");
    await expect(page.getByText(/Marks must be between 0 and/)).toBeVisible();
    await expect(page.getByRole("button", { name: "Save marks" })).toBeDisabled();

    await first.fill("17");
    await page.getByRole("button", { name: "Save marks" }).click();
    await expect(page.getByText("Marks saved.")).toBeVisible();
    await page.getByRole("button", { name: "Submit for approval" }).click();
    await expect(page.getByText(/Submitted by/)).toBeVisible();
    await page.getByRole("button", { name: "Approve", exact: true }).click();
    await expect(page.getByText(/Approved by/)).toBeVisible();
    await expect(first).toBeDisabled();
    await expect(first).toHaveValue("17");
    await context.close();
  });

  test("a report card prints as a PDF with the student, the subjects and the result as real text", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { route: "/exams" });
    await page.getByRole("tab", { name: "Report cards" }).click();
    const row = page.locator("tbody tr").first();
    const name = (await row.locator("td").nth(1).innerText()).split("\n")[0].trim();
    const overall = (await row.locator("td").nth(2).innerText()).trim();
    expect(overall).toMatch(/^\d+(\.\d+)?%$/);

    await row.getByRole("link", { name: "Print report card" }).click();
    await expect(page).toHaveURL(/\/exams\/report-cards\/print/);
    await expect(page.getByText(name).first()).toBeVisible();

    const { path } = await downloadFrom(page, () =>
      page.getByRole("button", { name: "Print / Save as PDF" }).click(),
    );
    expect(pdfPages(path)).toBe(1);
    const text = pdfText(path);
    expect(text).toContain(name);
    expect(text).toMatch(/DEMO .+ School/);
    expect(text).toMatch(/Report card/i);
    // The card prints the overall percentage to one decimal place.
    expect(text).toContain(Number(overall.replace("%", "")).toFixed(1));
    expect(text).toMatch(/Pass|Fail/);
    expect(text).not.toMatch(/�/);
    await context.close();
  });
});
