import { expect, test } from "@playwright/test";
import { call, tokenFor } from "../support/api";
import { openDesktop } from "../support/desktop";

interface StudentRow {
  id: string;
  student_number: string;
  full_name: string;
  father_name: string;
}

test.describe("students", () => {
  test("the list pages, searches and opens a student's profile", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { route: "/students" });
    await expect(page.getByRole("heading", { level: 1, name: "Students" })).toBeVisible();
    const rows = page.locator("tbody tr");
    const all = await call<{ items: StudentRow[]; total: number }>("/students?per_page=5", {
      token: await tokenFor("admin"),
    });
    expect(all.total).toBeGreaterThan(50);
    await expect(rows).toHaveCount(50);
    await page.getByRole("button", { name: "Next page" }).click();
    await expect(rows).toHaveCount(Math.min(50, all.total - 50));

    const student = all.items[0];
    await page.getByRole("searchbox", { name: /Search by name/ }).fill(student.student_number);
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toContainText(student.full_name);

    await rows.first().click();
    await expect(page.getByRole("heading", { level: 1, name: student.full_name })).toBeVisible();
    for (const tab of ["Details", "Guardians", "Fees", "Attendance", "Results"]) {
      await expect(page.getByRole("tab", { name: tab, exact: true })).toBeVisible();
    }
    await page.getByRole("tab", { name: "Guardians", exact: true }).click();
    await expect(page.getByRole("tabpanel")).toContainText(/Father|Mother|Guardian/);
    await context.close();
  });

  test("a new student is added with a guardian and is on the server", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { route: "/students" });
    await page.getByRole("button", { name: "Add student" }).click();
    await expect(page).toHaveURL(/\/students\/new/);

    // Saving an empty form says what is missing and saves nothing.
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page.getByText(/required/i).first()).toBeVisible();

    const name = `Zainab Testwala ${Date.now() % 100000}`;
    await page.getByLabel("Student name").fill(name);
    await page.getByLabel("Father's name").fill("Imran Testwala");
    await page.getByLabel("Date of birth").fill("2019-03-14");
    await page.getByLabel("Gender").selectOption({ label: "Female" });
    await page.getByLabel("Admission date").fill("2026-11-02");
    // The last section, so the sheets the other tests work on keep their own students.
    const section = page.getByLabel("Class / section");
    await section.selectOption({ index: (await section.locator("option").count()) - 1 });
    await page.getByRole("button", { name: "New guardian" }).click();
    await page.getByLabel("Name", { exact: true }).fill("Imran Testwala");
    await page.getByLabel("Phone", { exact: true }).fill("03001112233");
    await page.getByRole("button", { name: "Save", exact: true }).click();

    await expect(page.getByText(/^Student .+ added\.$/)).toBeVisible();
    await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();

    const found = await call<{ items: StudentRow[] }>(`/students?q=${encodeURIComponent(name)}`, {
      token: await tokenFor("admin"),
    });
    expect(found.items.map((s) => s.full_name)).toEqual([name]);
    expect(found.items[0].father_name).toBe("Imran Testwala");
    // The guardian typed on the same form is linked, with the phone as the school will dial it.
    await page.getByRole("tab", { name: "Guardians", exact: true }).click();
    await expect(page.getByRole("tabpanel")).toContainText("Imran Testwala");
    await expect(page.getByRole("tabpanel")).toContainText(/0?300[ -]?1112233/);
    await context.close();
  });
});
