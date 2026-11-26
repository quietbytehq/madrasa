import { expect, test } from "@playwright/test";
import { clippedText } from "../support/checks";
import { openDesktop } from "../support/desktop";

const SCREENS = [
  "/",
  "/students",
  "/students/new",
  "/attendance",
  "/exams",
  "/fees",
  "/reports",
  "/backup",
  "/settings",
];
const WIDTHS = [
  { width: 1024, height: 768 },
  { width: 1366, height: 768 },
  { width: 1920, height: 1080 },
];

test.describe("Urdu", () => {
  test("switching to Urdu from the user menu turns the whole app right-to-left, and it stays", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { route: "/" });
    await expect(page.locator("html")).toHaveAttribute("dir", "ltr");
    await page.getByRole("button", { name: "User menu" }).click();
    await page.getByRole("menuitem", { name: "اردو" }).click();

    const html = page.locator("html");
    await expect(html).toHaveAttribute("dir", "rtl");
    await expect(html).toHaveAttribute("lang", "ur");
    const nav = page.getByRole("navigation").first();
    await expect(nav).toContainText("طلبہ");
    await expect(nav).toContainText("فیس و مالیات");
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("ڈیش بورڈ");
    // Urdu is set in Noto Nastaliq Urdu, which the app carries itself.
    const font = await page.evaluate(() => getComputedStyle(document.body).fontFamily);
    expect(font).toContain("Noto Nastaliq Urdu");
    // (The font file arrives a moment after the switch: wait for it rather than look once.)
    await expect
      .poll(() => page.evaluate(() => document.fonts.check('16px "Noto Nastaliq Urdu"', "طلبہ")))
      .toBe(true);
    // The sidebar is on the right.
    const side = await nav.boundingBox();
    expect(side!.x).toBeGreaterThan(1366 / 2);

    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
    await page.getByRole("navigation").first().getByRole("link", { name: "طلبہ" }).click();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("طلبہ");
    await context.close();
  });

  test("the cut-off check itself catches a label that does not fit", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { lang: "ur", route: "/" });
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.evaluate(() => {
      const side = document.createElement("button");
      side.textContent = "طلبہ کی فہرست برآمد کریں";
      side.style.cssText = "width:40px;overflow:hidden;white-space:nowrap";
      const tall = document.createElement("label");
      tall.textContent = "ڈیٹا";
      tall.style.cssText = "display:block;height:10px;line-height:10px;overflow:hidden";
      document.querySelector("main")!.prepend(side, tall);
    });
    const found = await clippedText(page);
    expect(found.map((f) => f.why).sort()).toEqual(["cut at the side", "cut at the top or bottom"]);
    await context.close();
  });

  for (const viewport of WIDTHS) {
    test(`no Urdu text is cut off at ${viewport.width}×${viewport.height}`, async ({
      browser,
    }, testInfo) => {
      const { page, context } = await openDesktop(browser, { lang: "ur", viewport, route: "/" });
      const problems: Record<string, unknown> = {};
      for (const route of SCREENS) {
        await page.goto(new URL(route, page.url()).toString());
        await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
        await page.waitForLoadState("networkidle");
        const clipped = await clippedText(page);
        if (clipped.length) {
          problems[route] = clipped;
          await testInfo.attach(`urdu-${viewport.width}${route.replace(/\//g, "-")}.png`, {
            body: await page.screenshot({ fullPage: true }),
            contentType: "image/png",
          });
        }
      }
      expect(problems).toEqual({});
      await context.close();
    });
  }

  test("English is not cut off either at the smallest supported width", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { viewport: WIDTHS[0], route: "/" });
    const problems: Record<string, unknown> = {};
    for (const route of SCREENS) {
      await page.goto(new URL(route, page.url()).toString());
      await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
      await page.waitForLoadState("networkidle");
      const clipped = await clippedText(page);
      if (clipped.length) problems[route] = clipped;
    }
    expect(problems).toEqual({});
    await context.close();
  });
});
