import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { a11yProblems } from "../support/a11y";
import { clippedText, lowContrast } from "../support/checks";
import { openConsole } from "../support/console";
import { openDesktop } from "../support/desktop";
import { BACKUP_PASSWORD, HUB_ADMIN, PASSWORD, URLS } from "../support/env";
import { downloadFrom } from "../support/pdf";

const SHOTS = "e2e/test-results/screens";
const ESCROW_PASSPHRASE = "e2e escrow passphrase 1";

/** The text of every cell in a workbook (its shared strings). */
function workbookText(path: string): string {
  const files = execFileSync("unzip", ["-Z1", path], { encoding: "utf8" });
  expect(files).toContain("xl/workbook.xml");
  const part = files.includes("xl/sharedStrings.xml")
    ? "xl/sharedStrings.xml"
    : "xl/worksheets/sheet1.xml";
  return execFileSync("unzip", ["-p", path, part], { encoding: "utf8" });
}

test.describe("hub console", () => {
  test("signing in: a wrong password is refused, the right one shows the joined schools", async ({
    browser,
  }) => {
    const context = await browser.newContext({ viewport: { width: 1366, height: 768 } });
    const page = await context.newPage();
    await page.goto(URLS.hub);
    await page.getByLabel("Username").fill(HUB_ADMIN);
    await page.getByLabel("Password").fill("not-the-password");
    await page.locator('button[type="submit"]').click();
    await expect(page.getByRole("alert")).toContainText(/username or password/i);

    await page.getByLabel("Password").fill(PASSWORD);
    await page.locator('button[type="submit"]').click();
    await expect(page.getByRole("heading", { level: 1, name: "Schools" })).toBeVisible();
    const rows = page.locator("tbody tr");
    for (const school of ["DEMO International Campus", "DEMO North Campus", "DEMO South Campus"]) {
      await expect(rows.filter({ hasText: school })).toContainText(/Joined/i);
    }
    // One school's page: what is waiting, and the actions on it.
    await rows
      .filter({ hasText: "DEMO North Campus" })
      .getByRole("button", { name: "Open" })
      .click();
    const dialog = page.getByRole("dialog");
    await expect(dialog.getByRole("heading", { name: /DEMO North Campus/ })).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Send everything again" })).toBeVisible();
    await page.keyboard.press("Escape");

    await page.getByRole("button", { name: "Log out" }).click();
    await expect(page.getByLabel("Password")).toBeVisible();
    await page.goto(`${URLS.hub}/schools`);
    await expect(page.getByLabel("Password")).toBeVisible();
    await context.close();
  });

  test("reports: a report shows every school, and downloads as an Excel workbook", async ({
    browser,
  }) => {
    const { page, context } = await openConsole(browser);
    await page.getByRole("navigation").getByRole("link", { name: "Reports" }).click();
    await page.getByLabel("Report").selectOption({ label: "Enrolment by school and class" });
    await page.getByRole("button", { name: "Show" }).click();
    const table = page.locator("table").first();
    await expect(table).toContainText("DEMO North Campus");
    await expect(table).toContainText("DEMO South Campus");

    const { download, path } = await downloadFrom(page, () =>
      page.getByRole("button", { name: "Download Excel" }).click(),
    );
    expect(download.suggestedFilename()).toMatch(/\.xlsx$/);
    const text = workbookText(path);
    expect(text).toContain("DEMO North Campus");
    expect(text).toContain("DEMO International Campus");

    // Money is shown per currency, never added across them.
    await page
      .getByLabel("Report")
      .selectOption({ label: "Fees billed and collected by school and month" });
    await page.getByRole("button", { name: "Show" }).click();
    await expect(page.locator("table").first()).toContainText(/PKR|Rs/);
    const csv = await downloadFrom(page, () =>
      page.getByRole("button", { name: "Download CSV" }).click(),
    );
    expect(csv.download.suggestedFilename()).toMatch(/\.csv$/);
    await context.close();
  });

  test("escrow: a school joins, leaves its backup key, and head office releases it with a reason", async ({
    browser,
  }) => {
    // Once only: escrow is set up and the school joins, which cannot be done twice.
    test
      .info()
      .annotations.push({ type: "note", description: "not repeatable on the same servers" });
    test.skip(test.info().retry > 0, "the first try changed the servers; run scripts/e2e.sh again");
    test.setTimeout(180_000);
    const { page, context } = await openConsole(browser);

    // Head office sets up escrow.
    await page.getByRole("navigation").getByRole("link", { name: "Backup keys" }).click();
    await page.getByLabel("Escrow passphrase").fill(ESCROW_PASSPHRASE);
    await page.getByLabel("Passphrase again").fill(ESCROW_PASSPHRASE);
    await page.getByRole("button", { name: "Set up key escrow" }).click();
    await expect(
      page.getByText("No school has left its backup key yet.", { exact: false }),
    ).toBeVisible();

    // Head office invites the school.
    await page.getByRole("navigation").getByRole("link", { name: "Schools" }).click();
    await page.getByRole("button", { name: "Invite a school" }).click();
    const invite = page.getByRole("dialog");
    await invite.getByLabel("School's name").fill("E2E Gulshan School");
    await invite.getByRole("button", { name: "Create join code" }).click();
    await expect(invite).toContainText("Copy the code now");
    const code = /[A-Z0-9]{4}(-[A-Z0-9]{4}){2,}/.exec(await invite.innerText())![0];
    await page.keyboard.press("Escape");

    // The school joins from Settings, and leaves a sealed copy of its backup key.
    const school = await openDesktop(browser, { route: "/settings" });
    const s = school.page;
    await s.getByLabel("Hub address").fill(URLS.hub);
    await s.getByLabel("Join code").fill(code);
    await s.getByRole("button", { name: "Join the network" }).click();
    await expect(s.getByText(/^Joined .+\. The first upload has started\.$/)).toBeVisible({
      timeout: 30_000,
    });
    await expect(s.getByRole("heading", { name: "Backup key and head office" })).toBeVisible();
    await s.getByLabel(/Backup password/).fill(BACKUP_PASSWORD);
    await s.getByRole("button", { name: "Leave the key with head office" }).click();
    await expect(
      s.getByText("A sealed copy of the backup key is now with head office."),
    ).toBeVisible({ timeout: 30_000 });

    // Head office sees the school and its key, and releases it.
    await page.reload();
    await expect(
      page
        .locator("tbody tr")
        .filter({ hasText: "E2E Gulshan School" })
        .or(page.locator("tbody tr").filter({ hasText: "DEMO Gulshan Model School" }))
        .first(),
    ).toBeVisible();
    await page.getByRole("navigation").getByRole("link", { name: "Backup keys" }).click();
    const deposit = page
      .locator("li, tr")
      .filter({ hasText: /Gulshan/ })
      .first();
    await deposit.getByRole("button", { name: "Release key" }).click();
    const release = page.getByRole("dialog");
    await expect(release).toContainText("The release is recorded with your name and reason");
    // A wrong passphrase releases nothing.
    await release.getByLabel("Escrow passphrase").fill("not the passphrase at all");
    await release
      .getByLabel("Reason")
      .fill("E2E: the principal lost the backup password and the recovery key.");
    await release.getByRole("button", { name: "Release key" }).click();
    await expect(page.getByText("The escrow passphrase is incorrect.")).toBeVisible();
    await expect(release.getByLabel("Escrow passphrase")).toBeVisible();
    await release.getByLabel("Escrow passphrase").fill(ESCROW_PASSPHRASE);
    await release.getByRole("button", { name: "Release key" }).click();
    const released = page.getByRole("dialog");
    await expect(released).toContainText("Copy or write down this code now.");
    await released.getByRole("button", { name: "I have copied it" }).click();
    await expect(page.getByRole("heading", { name: "Releases" })).toBeVisible();
    await expect(page.locator("main")).toContainText("E2E: the principal lost the backup password");

    // The release is in the activity log, and the school is told at its next sync.
    await page.getByRole("navigation").getByRole("link", { name: "Activity log" }).click();
    await expect(page.locator("main")).toContainText(/escrow|backup key/i);
    await s.getByRole("button", { name: "Sync now" }).click();
    await expect(s.getByText("Head office has opened this school's backup key")).toBeVisible({
      timeout: 30_000,
    });
    await expect(s.locator("main")).toContainText("E2E: the principal lost the backup password");
    await school.context.close();
    await context.close();
  });

  test("passwords: a user changes their own with the current one; an administrator resets another's", async ({
    browser,
  }) => {
    // The administrator adds a user who may only read reports.
    const { page: admin, context: adminContext } = await openConsole(browser);
    await admin.goto(`${URLS.hub}/users`);
    await admin.getByRole("button", { name: "Add a user" }).click();
    let dialog = admin.getByRole("dialog");
    await dialog.getByLabel("Full name").fill("Sana Malik");
    await dialog.getByLabel("Username").fill("e2e.sana");
    await dialog.getByLabel("Password").fill("given-by-admin-1");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(admin.locator("tbody tr").filter({ hasText: "e2e.sana" })).toBeVisible();

    // She signs in and changes it to one only she knows.
    const signIn = async (page: import("@playwright/test").Page, password: string) => {
      await page.goto(URLS.hub);
      await page.getByLabel("Username").fill("e2e.sana");
      await page.getByLabel("Password").fill(password);
      await page.locator('button[type="submit"]').click();
    };
    const context = await browser.newContext({ viewport: { width: 1366, height: 768 } });
    const page = await context.newPage();
    await signIn(page, "given-by-admin-1");
    await expect(page.getByRole("heading", { level: 1, name: "Schools" })).toBeVisible();
    await page.getByRole("button", { name: "Change password" }).click();
    dialog = page.getByRole("dialog", { name: "Change password" });
    expect(await a11yProblems(page, "change password")).toEqual([]);
    await dialog.getByLabel("Current password").fill("not-what-she-was-given");
    await dialog.getByLabel("New password", { exact: true }).fill("only-sana-knows-1");
    await dialog.getByLabel("New password again").fill("only-sana-knows-1");
    await dialog.getByRole("button", { name: "Change password" }).click();
    await expect(dialog.getByRole("alert")).toContainText("current password is not right");
    await dialog.getByLabel("Current password").fill("given-by-admin-1");
    await dialog.getByRole("button", { name: "Change password" }).click();
    await expect(page.getByText("Password changed.")).toBeVisible();
    await expect(dialog).toBeHidden();
    // Still signed in here; the old password no longer signs in, the new one does.
    await page.reload();
    await expect(page.getByRole("heading", { level: 1, name: "Schools" })).toBeVisible();
    await page.getByRole("button", { name: "Log out" }).click();
    await signIn(page, "given-by-admin-1");
    await expect(page.getByRole("alert")).toContainText(/username or password/i);
    await signIn(page, "only-sana-knows-1");
    await expect(page.getByRole("heading", { level: 1, name: "Schools" })).toBeVisible();

    // She forgets it: the administrator sets a new one, which signs her out at once.
    await admin
      .locator("tbody tr")
      .filter({ hasText: "e2e.sana" })
      .getByRole("button", { name: "Edit" })
      .click();
    dialog = admin.getByRole("dialog");
    await dialog.getByLabel(/New password/).fill("reset-by-admin-2");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(dialog).toBeHidden();
    await page.reload();
    await expect(page.getByLabel("Password")).toBeVisible();
    await signIn(page, "reset-by-admin-2");
    await expect(page.getByRole("heading", { level: 1, name: "Schools" })).toBeVisible();

    // The administrator's own row offers no password box: theirs asks for the current one too.
    await admin
      .locator("tbody tr")
      .filter({ hasText: "(you)" })
      .getByRole("button", { name: "Edit" })
      .click();
    dialog = admin.getByRole("dialog");
    await expect(dialog.getByText(/asks for your current password/)).toBeVisible();
    await expect(dialog.getByLabel(/New password/)).toHaveCount(0);
    await context.close();
    await adminContext.close();
  });

  test("Urdu: right-to-left and nothing cut off; light and dark are readable", async ({
    browser,
  }, testInfo) => {
    mkdirSync(SHOTS, { recursive: true });
    const problems: Record<string, unknown> = {};
    for (const [lang, theme] of [
      ["ur", "light"],
      ["en", "dark"],
    ] as const) {
      const { page, context } = await openConsole(browser, { lang, theme });
      await expect(page.locator("html")).toHaveAttribute("dir", lang === "ur" ? "rtl" : "ltr");
      for (const route of ["/schools", "/reports", "/escrow", "/users", "/activity"]) {
        await page.goto(`${URLS.hub}${route}`);
        await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
        await page.waitForLoadState("networkidle");
        const file = `console-${theme}-${lang}${route.replace(/\//g, "-")}.png`;
        await testInfo.attach(file, {
          body: await page.screenshot({ path: `${SHOTS}/${file}` }),
          contentType: "image/png",
        });
        const clipped = await clippedText(page);
        const low = await lowContrast(page);
        const a11y = await a11yProblems(page, route);
        if (clipped.length || low.length || a11y.length)
          problems[`${lang} ${theme} ${route}`] = { clipped, low, a11y };
      }
      await context.close();
    }
    expect(problems).toEqual({});
  });
});
