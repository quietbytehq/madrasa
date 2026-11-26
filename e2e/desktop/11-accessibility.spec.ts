import { expect, test } from "@playwright/test";
import { a11yProblems, settled, walkForm, walkProblems } from "../support/a11y";
import { call, tokenFor } from "../support/api";
import { openDesktop } from "../support/desktop";

// Every main screen and every tab on it, checked by machine for what a keyboard or
// screen-reader user needs: names, labels, roles (support/a11y.ts).
const SCREENS = [
  "/",
  "/students",
  "/students/new",
  "/students/subjects",
  "/students/consents",
  "/students/sections",
  "/import",
  "/data-health",
  "/attendance",
  "/exams",
  "/fees",
  "/teachers",
  "/teachers/allocation",
  "/reports",
  "/backup",
  "/settings",
];

test.describe("Accessibility", () => {
  test("every main screen and tab has named controls, labelled fields and proper roles", async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const one = (
      await call<{ items: { id: string }[] }>("/students?page_size=1", {
        token: await tokenFor("admin"),
      })
    ).items[0];
    const { page, context } = await openDesktop(browser, { route: "/" });
    const problems: string[] = [];
    for (const route of [...SCREENS, `/students/${one.id}`, `/students/${one.id}/edit`]) {
      await page.goto(`http://127.0.0.1:1425${route}`);
      await settled(page);
      problems.push(...(await a11yProblems(page, route)));
      const tabs = page.getByRole("tab");
      for (let i = 1; i < (await tabs.count()); i++) {
        const name = (await tabs.nth(i).textContent())?.trim() ?? String(i);
        await tabs.nth(i).click();
        await settled(page);
        problems.push(...(await a11yProblems(page, `${route} › ${name}`)));
      }
    }
    expect([...new Set(problems)]).toEqual([]);
    await context.close();
  });

  test("the sign-in screen", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { user: null, route: "/login" });
    await page.getByLabel("Username").waitFor();
    expect(await a11yProblems(page, "/login")).toEqual([]);
    await context.close();
  });
  test("menus, the search box, Help and a payment dialog, opened, are as accessible", async ({
    browser,
  }) => {
    const token = await tokenFor("admin");
    const voucher = (
      await call<{ student_id: string; voucher_no: string }[]>(
        "/fees/vouchers?status=unpaid&limit=1",
        { token },
      )
    )[0];
    const { page, context } = await openDesktop(browser, {
      route: `/fees?tab=collect&student=${voucher.student_id}`,
    });
    await settled(page);
    const problems: string[] = [];
    await page.getByRole("button", { name: /^User menu/ }).click();
    await page.getByRole("menuitem").first().waitFor();
    problems.push(...(await a11yProblems(page, "user menu")));
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Notifications" }).click();
    problems.push(...(await a11yProblems(page, "notifications")));
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Help" }).click();
    await page.getByRole("dialog", { name: "Help" }).waitFor();
    problems.push(...(await a11yProblems(page, "help")));
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+k");
    await page.getByRole("dialog", { name: "Search" }).waitFor();
    problems.push(...(await a11yProblems(page, "search")));
    await page.keyboard.press("Escape");
    await page
      .locator("tbody tr", { hasText: voucher.voucher_no })
      .getByRole("button", { name: "Record payment" })
      .click();
    await page.getByRole("dialog", { name: "Record payment" }).waitFor();
    problems.push(...(await a11yProblems(page, "record payment")));
    expect([...new Set(problems)]).toEqual([]);
    await context.close();
  });

  test("the Standard edition's screens, and the same screens in Urdu", async ({ browser }) => {
    test.setTimeout(240_000);
    const token = await tokenFor("admin");
    await call("/settings/edition", { method: "PUT", token, body: { edition: "standard" } });
    try {
      const problems: string[] = [];
      const standard = await openDesktop(browser, { route: "/" });
      for (const route of [
        "/timetable",
        "/leave",
        "/admissions",
        "/library",
        "/transport",
        "/hostel",
        "/inventory",
      ]) {
        await standard.page.goto(`http://127.0.0.1:1425${route}`);
        await settled(standard.page);
        problems.push(...(await a11yProblems(standard.page, route)));
        const tabs = standard.page.getByRole("tab");
        for (let i = 1; i < (await tabs.count()); i++) {
          const name = (await tabs.nth(i).textContent())?.trim() ?? String(i);
          await tabs.nth(i).click();
          await settled(standard.page);
          problems.push(...(await a11yProblems(standard.page, `${route} › ${name}`)));
        }
      }
      await standard.context.close();
      const urdu = await openDesktop(browser, { lang: "ur", route: "/" });
      for (const route of ["/", "/students", "/students/new", "/fees", "/attendance", "/backup"]) {
        await urdu.page.goto(`http://127.0.0.1:1425${route}`);
        await settled(urdu.page);
        problems.push(...(await a11yProblems(urdu.page, `ur ${route}`)));
      }
      await urdu.context.close();
      expect([...new Set(problems)]).toEqual([]);
    } finally {
      await call("/settings/edition", { method: "PUT", token, body: { edition: "lite" } });
    }
  });
});

test.describe("Keyboard", () => {
  test("the first Tab offers to skip to the content, and takes the keyboard there", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { route: "/students" });
    await settled(page);
    await page.keyboard.press("Tab");
    const skip = page.getByRole("link", { name: "Skip to content" });
    await expect(skip).toBeFocused();
    await expect(skip).toBeInViewport();
    await page.keyboard.press("Enter");
    await expect(page.locator("main#main")).toBeFocused();
    await context.close();
  });

  test("tabs are walked with the arrow keys, mirrored in Urdu", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { route: "/fees" });
    await settled(page);
    const tab = (name: string) => page.getByRole("tab", { name, exact: true });
    await tab("Vouchers").focus();
    await page.keyboard.press("ArrowRight");
    await expect(tab("Collect")).toBeFocused();
    await expect(tab("Collect")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByLabel("Find a student by name or number")).toBeVisible();
    await page.keyboard.press("End");
    await expect(tab("Setup")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("ArrowRight");
    await expect(tab("Vouchers")).toBeFocused();
    await page.keyboard.press("ArrowLeft");
    await expect(tab("Setup")).toBeFocused();
    await page.keyboard.press("Home");
    await expect(tab("Vouchers")).toHaveAttribute("aria-selected", "true");
    await context.close();

    // Right to left the strip is mirrored, and so are the arrows: Left goes on.
    const ur = await openDesktop(browser, { lang: "ur", route: "/fees" });
    await settled(ur.page);
    const tabs = ur.page.getByRole("tab");
    await tabs.nth(0).focus();
    await ur.page.keyboard.press("ArrowLeft");
    await expect(tabs.nth(1)).toBeFocused();
    await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
    await ur.page.keyboard.press("ArrowRight");
    await expect(tabs.nth(0)).toBeFocused();
    await ur.context.close();
  });

  test("the user menu opens, is walked and is closed from the keyboard", async ({ browser }) => {
    const { page, context } = await openDesktop(browser, { route: "/" });
    await settled(page);
    const opener = page.getByRole("button", { name: /^User menu/ });
    await opener.focus();
    await page.keyboard.press("Enter");
    const items = page.getByRole("menuitem");
    await expect(items.first()).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(items.nth(1)).toBeFocused();
    await page.keyboard.press("End");
    await expect(items.last()).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(items.first()).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(items).toHaveCount(0);
    await expect(opener).toBeFocused();
    await context.close();
  });

  test("a dialog takes the keyboard, keeps it, and gives it back when closed", async ({
    browser,
  }) => {
    const token = await tokenFor("admin");
    const voucher = (
      await call<{ student_id: string; voucher_no: string }[]>(
        "/fees/vouchers?status=unpaid&limit=1",
        { token },
      )
    )[0];
    const { page, context } = await openDesktop(browser, {
      route: `/fees?tab=collect&student=${voucher.student_id}`,
    });
    await settled(page);
    const opener = page
      .locator("tbody tr", { hasText: voucher.voucher_no })
      .getByRole("button", { name: "Record payment" });
    await opener.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Record payment" });
    await expect(dialog).toBeVisible();
    const inside = () => page.evaluate(() => !!document.activeElement?.closest('[role="dialog"]'));
    expect(await inside()).toBe(true);
    // Tab goes round inside the dialog, forwards and backwards, and never behind it.
    for (let i = 0; i < 14; i++) {
      await page.keyboard.press(i % 5 === 4 ? "Shift+Tab" : "Tab");
      expect(await inside()).toBe(true);
    }
    const walk = await walkForm(page, '[role="dialog"] form');
    expect(walkProblems("record payment", walk)).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(opener).toBeFocused();
    await context.close();
  });

  test("every form is filled from the keyboard: all fields reached in order, focus visible, each with a label", async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const one = (
      await call<{ items: { id: string }[] }>("/students?page_size=1", {
        token: await tokenFor("admin"),
      })
    ).items[0];
    const problems: string[] = [];
    let forms = 0;
    for (const lang of ["en", "ur"] as const) {
      const { page, context } = await openDesktop(browser, { lang, route: "/" });
      const routes = [
        "/students/new",
        `/students/${one.id}/edit`,
        "/students",
        "/attendance?tab=calendar",
        "/exams?tab=setup",
        "/fees?tab=setup",
        "/fees?tab=vouchers",
        "/teachers",
        "/backup",
        "/settings",
        "/import",
      ];
      for (const route of lang === "en" ? routes : routes.slice(0, 2)) {
        await page.goto(`http://127.0.0.1:1425${route}`);
        await settled(page);
        const count = await page.locator("main form").count();
        for (let i = 0; i < count; i++) {
          await page.evaluate((i) => {
            document.querySelectorAll("main form").forEach((f, n) => {
              f.toggleAttribute("data-walking", n === i);
            });
          }, i);
          const walk = await walkForm(page, "main form[data-walking]");
          if (walk.controls === 0) continue;
          forms += 1;
          problems.push(...walkProblems(`${lang} ${route} form ${i + 1}`, walk, lang === "ur"));
        }
      }
      await context.close();
    }
    const login = await openDesktop(browser, { user: null, route: "/login" });
    await login.page.getByLabel("Username").waitFor();
    problems.push(...walkProblems("/login", await walkForm(login.page, "form")));
    await login.context.close();
    test.info().annotations.push({ type: "forms walked", description: String(forms) });
    console.log(`forms walked with Tab: ${forms}`);
    expect(forms).toBeGreaterThan(10);
    expect(problems).toEqual([]);
  });
});
