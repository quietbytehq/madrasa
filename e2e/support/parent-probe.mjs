#!/usr/bin/env node
// What the parent app shows after signing in through its own screens (to write tests against).
import { chromium } from "@playwright/test";
const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: 390, height: 844 },
  isMobile: true,
  hasTouch: true,
});
const page = await context.newPage();
const dump = async (label) => {
  await page.waitForTimeout(1200);
  const seen = await page.evaluate(() => ({
    url: location.href,
    storage: Object.fromEntries(
      Object.keys(localStorage).map((k) => [k, localStorage.getItem(k).slice(0, 30)]),
    ),
    text: document.body.innerText.replace(/\s+/g, " ").slice(0, 700),
    buttons: [...document.querySelectorAll("button,a")]
      .map((b) => (b.getAttribute("aria-label") || b.innerText).replace(/\s+/g, " ").trim())
      .filter(Boolean)
      .slice(0, 30),
    fields: [...document.querySelectorAll("label")].map((l) =>
      l.innerText.replace(/\s+/g, " ").trim(),
    ),
    testids: [
      ...new Set(
        [...document.querySelectorAll("[data-testid]")].map((e) => e.getAttribute("data-testid")),
      ),
    ],
  }));
  console.log(`### ${label}\n${JSON.stringify(seen, null, 1)}`);
};
page.on(
  "console",
  (m) => m.type() === "error" && console.log("console error:", m.text().slice(0, 200)),
);
await page.goto("http://127.0.0.1:1445/");
await dump("start");
await page
  .getByRole("button", { name: "Sign in" })
  .or(page.getByRole("link", { name: "Sign in" }))
  .first()
  .click();
await dump("sign in");
await page.getByLabel("School network address").fill("http://localhost:47880");
await page.getByLabel("Phone number").fill("+923001234567");
await page.getByLabel("Password").fill(process.env.E2E_PASSWORD ?? "e2e-password-1");
await page.getByRole("button", { name: "Sign in" }).click();
await dump("children");
await page.locator("[data-testid^=child-]").first().click();
await dump("child");
for (const tab of ["Attendance", "Results", "Fees", "Notices", "Messages"]) {
  await page.getByRole("navigation").getByText(tab, { exact: true }).click();
  await dump(tab);
}
await browser.close();
