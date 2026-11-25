import { expect, test } from "@playwright/test";
import { ADMIN, PASSWORD } from "../support/env";
import { openDesktop, signIn } from "../support/desktop";

test.describe("login", () => {
  test("a wrong password is refused in plain words, the right one opens the dashboard", async ({
    browser,
  }) => {
    const { page, context } = await openDesktop(browser, { user: null });
    await expect(page.getByRole("button", { name: "Log in" })).toBeVisible();

    await signIn(page, ADMIN, "not-the-password");
    await expect(page.getByRole("alert")).toContainText("The username or password is incorrect.");
    await expect(page).toHaveURL(/\/login/);

    await signIn(page, ADMIN, PASSWORD);
    await expect(page.getByRole("heading", { level: 1, name: "Dashboard" })).toBeVisible();
    await expect(page.getByRole("navigation", { name: "Modules" })).toBeVisible();
    await expect(page.getByText("DEMO DATA")).toBeVisible();
    // The session is kept: a reload stays signed in.
    await page.reload();
    await expect(page.getByRole("heading", { level: 1, name: "Dashboard" })).toBeVisible();

    // Logging out goes back to the login screen, and the app stays out.
    await page.getByRole("button", { name: "User menu" }).click();
    await page.getByRole("menuitem", { name: "Log out" }).click();
    await expect(page.getByRole("button", { name: "Log in" })).toBeVisible();
    await page.goto(page.url().replace(/\/login.*/, "/students"));
    await expect(page.getByRole("button", { name: "Log in" })).toBeVisible();
    await context.close();
  });
});
