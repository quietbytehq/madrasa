import { execFileSync } from "node:child_process";
import { expect, test } from "@playwright/test";
import { call, tokenFor } from "../support/api";
import { openDesktop } from "../support/desktop";
import { downloadFrom } from "../support/pdf";

/** The names of a workbook's sheets, in order. */
function sheetNames(path: string): string[] {
  const xml = execFileSync("unzip", ["-p", path, "xl/workbook.xml"], { encoding: "utf8" });
  return [...xml.matchAll(/<sheet [^>]*name="([^"]+)"/g)].map((m) => m[1]);
}

/** All the text of a workbook written cell by cell (its inline strings and shared strings). */
function workbookText(path: string): string {
  return execFileSync("unzip", ["-p", path, "xl/worksheets/*.xml", "xl/sharedStrings.xml"], {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
    stdio: ["ignore", "pipe", "ignore"],
  });
}

interface Student {
  id: string;
  student_number: string;
  full_name: string;
}

test.describe("pilot onboarding", () => {
  test("a template comes with its Read me in English and Urdu", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { route: "/import" });
    await page.getByRole("button", { name: /^Guardians/ }).click();
    const { download, path } = await downloadFrom(page, () =>
      page.getByRole("button", { name: "Excel template" }).click(),
    );
    expect(download.suggestedFilename()).toBe("madrasa-guardians-template.xlsx");
    expect(sheetNames(path)).toEqual(["Guardians", "Read me", "ہدایات"]);
    const text = workbookText(path);
    expect(text).toContain("Nothing is saved until you confirm");
    expect(text).toContain("یہ فائل کیسے بھریں");
    await context.close();
  });

  test("guardians are imported after a dry run that writes nothing until it is confirmed", async ({
    browser,
  }) => {
    const token = await tokenFor("admin");
    const list = await call<{ items: Student[] }>("/students?per_page=5", { token });
    const [first, second] = list.items;
    const tag = Date.now() % 100000;
    const father = `Tariq Onboard ${tag}`;
    const csv = [
      "Student number,Guardian name,Relationship,Phone,Primary contact",
      `${first.student_number},${father},uncle,0300-55${String(tag).padStart(5, "0")},no`,
      `${second.student_number},${father},uncle,0300-55${String(tag).padStart(5, "0")},no`,
      `NO-SUCH-NUMBER,Nobody At All,father,0300-0000000,no`,
    ].join("\n");
    const linksBefore = (await call<{ guardians: unknown[] }>(`/students/${first.id}`, { token }))
      .guardians.length;

    const { page, context } = await openDesktop(browser, { route: "/import" });
    await page.getByRole("button", { name: /^Guardians/ }).click();
    await page
      .getByLabel("Choose a .xlsx or .csv file")
      .setInputFiles({ name: "guardians.csv", mimeType: "text/csv", buffer: Buffer.from(csv) });
    await expect(page.getByText("3 rows found in guardians.csv")).toBeVisible();
    await page.getByRole("button", { name: "Check the file" }).click();

    // The dry run: what will happen, and why one row cannot be imported.
    const dry = page.getByTestId("dry-run");
    await expect(dry).toContainText("Nothing has been saved yet");
    await expect(page.getByTestId("dry-create")).toContainText("2");
    await expect(page.getByTestId("dry-error")).toContainText("1");
    await expect(page.getByTestId("dry-reasons")).toContainText(
      "No student with this number or legacy ID",
    );
    const report = await downloadFrom(page, () =>
      dry.getByRole("button", { name: "Download dry-run report" }).click(),
    );
    expect(report.download.suggestedFilename()).toMatch(/^madrasa-guardians-dry-run-.*\.xlsx$/);
    expect(sheetNames(report.path)).toEqual([
      "Summary",
      "Rows",
      "Missing classes, sections",
      "Likely duplicates",
    ]);
    // Looking, downloading and opening the confirmation have written nothing.
    await page.getByRole("button", { name: "Import 2 rows" }).click();
    const confirm = page.getByRole("dialog", { name: "Import now?" });
    await expect(confirm).toContainText("2 will be created");
    await expect(confirm).toContainText("1 cannot be imported and will be left out");
    expect(
      (await call<{ guardians: unknown[] }>(`/students/${first.id}`, { token })).guardians,
    ).toHaveLength(linksBefore);

    await confirm.getByRole("button", { name: "Import now" }).click();
    await expect(page.getByRole("status").filter({ hasText: "Imported:" })).toContainText(
      "Imported: 2 added, 0 updated, 1 skipped.",
    );
    // One guardian record, linked to both children.
    const after = await call<{ guardians: { id: string; full_name: string }[] }>(
      `/students/${first.id}`,
      { token },
    );
    const mine = after.guardians.find((g) => g.full_name === father)!;
    expect(mine).toBeTruthy();
    const other = await call<{ guardians: { id: string }[] }>(`/students/${second.id}`, { token });
    expect(other.guardians.map((g) => g.id)).toContain(mine.id);
    await context.close();
  });

  test("data health lists what needs attention and leads to the record", async ({ browser }) => {
    // A child with no class: the first thing the page must find.
    const token = await tokenFor("admin");
    const name = `Loose Child ${Date.now() % 100000}`;
    const loose = await call<Student>("/students", {
      method: "POST",
      token,
      body: {
        full_name: name,
        father_name: "Nobody Yet",
        date_of_birth: "2016-02-02",
        gender: "female",
        admission_date: "2026-11-02",
      },
    });
    const { page, context } = await openDesktop(browser, { route: "/import" });
    await page.getByRole("button", { name: "Data health" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Data health" })).toBeVisible();
    await expect(page.getByTestId("health-summary")).toContainText(/Needs attention/);
    const noSection = page.getByTestId("health-students_without_section");
    await expect(noSection).toContainText("get no fee voucher");
    await expect(page.getByTestId("health-students_without_guardian")).toContainText(name);
    await noSection.getByRole("link", { name }).click();
    await expect(page).toHaveURL(new RegExp(`/students/${loose.id}$`));
    await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
    await context.close();
  });

  test("Export everything downloads the whole school as one workbook, without secrets", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { route: "/backup" });
    const { download, path } = await downloadFrom(page, () =>
      page.getByRole("button", { name: "Export everything" }).click(),
    );
    expect(download.suggestedFilename()).toMatch(/^madrasa-.+-everything-\d{4}-\d{2}-\d{2}\.xlsx$/);
    await expect(page.getByText(/It holds every record of the school/)).toBeVisible();
    const names = sheetNames(path);
    expect(names[0]).toBe("About this file");
    for (const sheet of [
      "Students",
      "Guardians",
      "Staff",
      "Fee vouchers",
      "Fee receipts",
      "Attendance",
      "Marks",
      "Settings",
    ]) {
      expect(names).toContain(sheet);
    }
    expect(names.filter((n) => /session|api_key|sync_/.test(n))).toEqual([]);
    const text = workbookText(path);
    const one = (
      await call<{ items: Student[] }>("/students?per_page=1", { token: await tokenFor("admin") })
    ).items[0];
    expect(text).toContain(one.student_number);
    expect(text).not.toContain("$argon2");
    await context.close();
  });
});
