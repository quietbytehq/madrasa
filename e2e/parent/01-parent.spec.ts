import { mkdirSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { clippedText, lowContrast } from "../support/checks";
import { PARENT_PHONE, PASSWORD, URLS } from "../support/env";
import { openParent } from "../support/parent";

const SHOTS = "e2e/test-results/screens";
const SECTIONS = ["attendance", "results", "fees", "notices", "messages"] as const;

test.describe("parent app on a phone (390×844)", () => {
  test("signing in: a wrong password is refused in plain words, the right one lists the children", async ({
    browser,
  }) => {
    const { page, context } = await openParent(browser, { signedIn: false });
    await expect(page.getByRole("heading", { name: "Welcome" })).toBeVisible();
    await page
      .getByRole("link", { name: "Sign in" })
      .or(page.getByRole("button", { name: "Sign in" }))
      .first()
      .click();

    // An address that would send the password in the clear is refused before anything is sent.
    await page.getByLabel("School network address").fill("http://hub.example.org");
    await page.getByLabel("Phone number").fill(PARENT_PHONE);
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByTestId("sign-in").click();
    await expect(page.getByRole("alert")).toContainText(
      "This does not look like a school network address.",
    );

    await page.getByLabel("School network address").fill(URLS.hub);
    await page.getByLabel("Password").fill("not-the-password");
    await page.getByTestId("sign-in").click();
    await expect(page.getByRole("alert")).toContainText(
      "The phone number or password is not right.",
    );

    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByTestId("sign-in").click();
    await expect(page.getByRole("heading", { name: "My children" })).toBeVisible();
    await expect(page.locator('[data-testid^="child-"]')).toHaveCount(5);
    await expect(page.getByText(`Signed in as ${PARENT_PHONE}`)).toBeVisible();

    // Signed in after a reload; signed out for good after Sign out.
    await page.reload();
    await expect(page.getByRole("heading", { name: "My children" })).toBeVisible();
    await page.getByTestId("sign-out").click();
    await expect(page.getByRole("heading", { level: 1, name: "Sign in" })).toBeVisible();
    await expect(page.getByLabel("Password")).toHaveValue("");
    await page.reload();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(/Sign in|Welcome/);
    await expect(page.locator('[data-testid^="child-"]')).toHaveCount(0);
    await context.close();
  });

  test("a child's attendance, results, fees and notices, each with when the school last sent them", async ({
    browser,
  }) => {
    const { page, context } = await openParent(browser);
    await page.locator('[data-testid^="child-"]').first().click();
    await expect(page.getByTestId("tile-attendance")).toContainText(/\d+ of \d+ days this year/);
    await expect(page.getByTestId("tile-fees")).toContainText(/PKR [\d,]+\.\d{2}/);
    await expect(page.getByTestId("last-updated")).toContainText("Last updated by the school");
    const nav = page.getByRole("navigation", { name: "Sections" });

    await nav.getByText("Attendance", { exact: true }).click();
    await expect(page.getByTestId("year-percent")).toHaveText(/\d+(\.\d)?%/);
    const month = await page.getByTestId("month").innerText();
    await page.getByTestId("month-previous").click();
    await expect(page.getByTestId("month")).not.toHaveText(month);

    await nav.getByText("Results", { exact: true }).click();
    await expect(page.getByTestId("assessment-0")).toContainText(/Mathematics\s*[\d.]+ \/ \d+/);
    await expect(page.getByTestId("position-0")).toHaveText(/Position \d+ of \d+ in the class/);

    await nav.getByText("Fees", { exact: true }).click();
    await expect(page.getByTestId("balance")).toHaveText(/PKR [\d,]+\.\d{2}/);
    await expect(
      page
        .getByText("Fees are paid at the school. This app only shows them.")
        .or(page.getByText("Nothing is due.")),
    ).toBeVisible();
    await expect(page.locator('[data-testid^="voucher-"]').first()).toContainText("Total due");

    await nav.getByText("Notices", { exact: true }).click();
    await expect(page.locator('[data-testid^="notice-"]').first()).toBeVisible();
    await context.close();
  });

  test("a message to the class teacher is sent and stays in the conversation", async ({
    browser,
  }) => {
    const { page, context } = await openParent(browser);
    await page.locator('[data-testid^="child-"]').first().click();
    await page
      .getByRole("navigation", { name: "Sections" })
      .getByText("Messages", { exact: true })
      .click();
    await expect(page.getByTestId("teacher")).not.toBeEmpty();
    const text = `Will she need her sports kit on Friday? (${Date.now() % 100000})`;
    await page.getByTestId("message-text").fill(text);
    await page.getByTestId("send").click();
    await expect(page.getByText(text)).toBeVisible();
    await expect(page.getByTestId("message-text")).toHaveValue("");
    await page.reload();
    await expect(page.getByText(text)).toBeVisible();
    await context.close();
  });

  test("the connection badge stays Connected while the hub answers, and says so when it does not", async ({
    browser,
  }) => {
    test.setTimeout(150_000);
    const { page, context } = await openParent(browser);
    const link = page.getByTestId("hub-link");
    await expect(link).toHaveText("Connected");
    // The app asks the hub every half minute; a refused answer once showed as "Reconnecting…".
    await page.waitForTimeout(40_000);
    await expect(link).toHaveText("Connected");

    await context.route(`${URLS.hub}/**`, (route) => route.abort("internetdisconnected"));
    await page.locator('[data-testid^="child-"]').first().click();
    await expect(link).toHaveText(/Reconnecting…|Offline/, { timeout: 20_000 });
    await expect(link).toHaveText("Offline", { timeout: 60_000 });
    await context.unroute(`${URLS.hub}/**`);
    await expect(link).toHaveText("Connected", { timeout: 30_000 });
    await context.close();
  });

  for (const viewport of [
    { width: 390, height: 844 },
    { width: 360, height: 740 },
  ]) {
    test(`Urdu at ${viewport.width}×${viewport.height}: right-to-left, nothing cut off`, async ({
      browser,
    }, testInfo) => {
      const { page, context } = await openParent(browser, { lang: "ur", viewport });
      await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
      await expect(page.locator("html")).toHaveAttribute("lang", "ur");
      const problems: Record<string, unknown> = {};
      const check = async (name: string) => {
        await page.waitForLoadState("networkidle");
        const clipped = await clippedText(page);
        if (clipped.length) {
          problems[name] = clipped;
          await testInfo.attach(`parent-ur-${viewport.width}-${name}.png`, {
            body: await page.screenshot({ fullPage: true }),
            contentType: "image/png",
          });
        }
      };
      await expect(page.locator('[data-testid^="child-"]').first()).toBeVisible();
      await check("children");
      await page.locator('[data-testid^="child-"]').first().click();
      await expect(page.getByTestId("tile-attendance")).toBeVisible();
      await check("home");
      const base = page.url();
      for (const section of SECTIONS) {
        await page.goto(`${base}/${section}`);
        await expect(page.getByTestId("last-updated")).toBeVisible();
        await check(section);
      }
      expect(problems).toEqual({});
      await context.close();
    });
  }

  test("switching to Urdu from the welcome screen", async ({ browser }) => {
    const { page, context } = await openParent(browser, { signedIn: false });
    await page.getByRole("button", { name: "اردو" }).click();
    await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
    await expect(page.getByRole("heading", { level: 1 })).not.toHaveText("Welcome");
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
    await context.close();
  });

  for (const theme of ["light", "dark"] as const) {
    test(`${theme} theme: every screen is readable (WCAG AA contrast)`, async ({
      browser,
    }, testInfo) => {
      mkdirSync(SHOTS, { recursive: true });
      const { page, context } = await openParent(browser, { theme });
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      const problems: Record<string, unknown> = {};
      const check = async (name: string) => {
        await page.waitForLoadState("networkidle");
        const file = `parent-${theme}-${name}.png`;
        const shot = await page.screenshot({ path: `${SHOTS}/${file}` });
        await testInfo.attach(file, { body: shot, contentType: "image/png" });
        const low = await lowContrast(page);
        if (low.length) problems[name] = low;
      };
      await expect(page.locator('[data-testid^="child-"]').first()).toBeVisible();
      await check("children");
      await page.locator('[data-testid^="child-"]').first().click();
      await expect(page.getByTestId("tile-attendance")).toBeVisible();
      await check("home");
      const base = page.url();
      for (const section of SECTIONS) {
        await page.goto(`${base}/${section}`);
        await expect(page.getByTestId("last-updated")).toBeVisible();
        await check(section);
      }
      expect(problems).toEqual({});
      await context.close();
    });
  }
});
