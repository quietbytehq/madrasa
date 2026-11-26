#!/usr/bin/env node
// What the hub console shows after signing in (to write tests against).
//   node e2e/support/console-probe.mjs /schools [--click "Name"]...
import { chromium } from "@playwright/test";
const args = process.argv.slice(2);
const route = args.find((a) => a.startsWith("/")) ?? "/";
const clicks = args.flatMap((a, i) => (a === "--click" ? [args[i + 1]] : []));
const browser = await chromium.launch();
const page = await (await browser.newContext({ viewport: { width: 1366, height: 768 } })).newPage();
await page.goto("http://localhost:47880/");
await page.waitForTimeout(800);
if (await page.getByLabel("Password").count()) {
  await page.getByLabel("Username").fill("hub.admin");
  await page.getByLabel("Password").fill(process.env.E2E_PASSWORD ?? "e2e-password-1");
  await page.getByRole("button", { name: /log in|sign in/i }).click();
  await page.waitForTimeout(1000);
}
if (route !== "/") await page.goto(`http://localhost:47880${route}`);
await page.waitForTimeout(1200);
for (const name of clicks) {
  await page
    .getByRole("button", { name, exact: true })
    .or(page.getByRole("link", { name, exact: true }))
    .or(page.getByRole("tab", { name, exact: true }))
    .or(page.getByText(name, { exact: true }))
    .first()
    .click();
  await page.waitForTimeout(1200);
}
const seen = await page.evaluate(() => {
  const root = document.querySelector("[role=dialog]") ?? document.body;
  const text = (e) =>
    (e.getAttribute("aria-label") || e.innerText || "").replace(/\s+/g, " ").trim();
  const list = (sel) =>
    [...new Set([...root.querySelectorAll(sel)].map(text).filter(Boolean))].slice(0, 40);
  return {
    where: location.pathname + (document.querySelector("[role=dialog]") ? " (dialog)" : ""),
    headings: list("h1,h2,h3"),
    nav: list("nav a"),
    buttons: list("button"),
    links: list("main a").slice(0, 12),
    fields: [...root.querySelectorAll("label")]
      .map((l) => l.innerText.replace(/\s+/g, " ").trim())
      .filter(Boolean)
      .slice(0, 30),
    columns: list("th"),
    rows: root.querySelectorAll("tbody tr").length,
    firstRows: [...root.querySelectorAll("tbody tr")]
      .slice(0, 3)
      .map((r) => r.innerText.replace(/\s+/g, " ").slice(0, 200)),
    text: (document.querySelector("main") ?? document.body).innerText
      .replace(/\s+/g, " ")
      .slice(0, 500),
    testids: [
      ...new Set(
        [...document.querySelectorAll("[data-testid]")].map((e) => e.getAttribute("data-testid")),
      ),
    ].slice(0, 40),
  };
});
console.log(JSON.stringify(seen, null, 1));
await browser.close();
