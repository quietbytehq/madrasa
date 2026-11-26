import type { Browser, BrowserContext, Page } from "@playwright/test";
import { HUB_ADMIN, PASSWORD, URLS } from "./env";

/** The hub console, signed in through its own login page. */
export async function openConsole(
  browser: Browser,
  { lang = "en", theme = "light" }: { lang?: "en" | "ur"; theme?: "light" | "dark" } = {},
): Promise<{ page: Page; context: BrowserContext }> {
  const context = await browser.newContext({
    viewport: { width: 1366, height: 768 },
    colorScheme: theme,
    acceptDownloads: true,
  });
  await context.addInitScript(
    ({ lang, theme }) => {
      if (sessionStorage.getItem("e2e.seeded")) return;
      sessionStorage.setItem("e2e.seeded", "1");
      localStorage.setItem("madrasa.locale", lang);
      localStorage.setItem("madrasa.theme", theme);
    },
    { lang, theme },
  );
  const page = await context.newPage();
  await page.goto(URLS.hub);
  await page.locator('input[type="text"], input:not([type])').first().fill(HUB_ADMIN);
  await page.locator('input[type="password"]').fill(PASSWORD);
  await page.locator('button[type="submit"]').click();
  await page.getByRole("navigation").first().waitFor();
  return { page, context };
}
