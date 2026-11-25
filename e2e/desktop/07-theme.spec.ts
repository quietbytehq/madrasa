import { mkdirSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { call, tokenFor } from "../support/api";
import { lowContrast } from "../support/checks";
import { openDesktop } from "../support/desktop";

const SCREENS = ["/", "/students", "/attendance", "/exams", "/fees", "/reports", "/settings"];
const SHOTS = "e2e/test-results/screens";

test.describe("light and dark", () => {
  test("the user menu switches theme, and the choice is kept", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { theme: "light", route: "/" });
    const html = page.locator("html");
    await expect(html).toHaveAttribute("data-theme", "light");
    const lightBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);

    await page.getByRole("button", { name: "User menu" }).click();
    await page.getByRole("menuitem", { name: "Dark" }).click();
    await expect(html).toHaveAttribute("data-theme", "dark");
    const darkBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(darkBg).not.toBe(lightBg);
    await page.reload();
    await expect(html).toHaveAttribute("data-theme", "dark");

    // "Follow system" takes the PC's setting.
    await page.getByRole("button", { name: "User menu" }).click();
    await page.getByRole("menuitem", { name: "Follow system" }).click();
    await page.emulateMedia({ colorScheme: "dark" });
    await expect(html).toHaveAttribute("data-theme", "dark");
    await page.emulateMedia({ colorScheme: "light" });
    await expect(html).toHaveAttribute("data-theme", "light");
    await context.close();
  });

  for (const theme of ["light", "dark"] as const) {
    for (const lang of ["en", "ur"] as const) {
      test(`${theme} theme, ${lang}: every main screen is readable (WCAG AA contrast)`, async ({
        browser,
      }, testInfo) => {
        mkdirSync(SHOTS, { recursive: true });
        const { page, context } = await openDesktop(browser, { theme, lang, route: "/" });
        const problems: Record<string, unknown> = {};
        for (const route of SCREENS) {
          await page.goto(new URL(route, page.url()).toString());
          await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
          await page.waitForLoadState("networkidle");
          const name = `${theme}-${lang}${route === "/" ? "-dashboard" : route.replace(/\//g, "-")}.png`;
          const shot = await page.screenshot({ path: `${SHOTS}/${name}` });
          await testInfo.attach(name, { body: shot, contentType: "image/png" });
          const low = await lowContrast(page);
          if (low.length) problems[route] = low;
        }
        expect(problems).toEqual({});
        await context.close();
      });
    }
  }

  test("a printed voucher is light even when the app is dark", async ({ browser }) => {
    const [voucher] = await call<{ id: string }[]>("/fees/vouchers?per_page=1", {
      token: await tokenFor("admin"),
    });
    const { page, context } = await openDesktop(browser, {
      theme: "dark",
      route: `/fees/vouchers/${voucher.id}/print`,
    });
    await expect(page.getByText("School copy")).toBeVisible();
    await page.emulateMedia({ media: "print" });
    const paper = await page.evaluate(() => {
      const area = document.querySelector(".print-area")!;
      const s = getComputedStyle(area);
      return { color: s.color, background: getComputedStyle(document.body).backgroundColor };
    });
    // Dark ink on white paper.
    expect(paper.background).toMatch(/rgb\(255, 255, 255\)|rgba\(0, 0, 0, 0\)/);
    const [r, g, b] = paper.color.match(/\d+/g)!.map(Number);
    expect(r + g + b).toBeLessThan(200);
    await context.close();
  });
});
