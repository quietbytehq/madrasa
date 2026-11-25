#!/usr/bin/env node
// List what a screen offers (headings, tabs, buttons, fields, table headings), to write tests
// against. The servers must be running (scripts/e2e.sh --keep).
//   node e2e/support/probe.mjs /students [--api client] [--user demo.x] [--click "Add student"]...
import { chromium } from "@playwright/test";

const args = process.argv.slice(2);
const route = args.find((a) => a.startsWith("/")) ?? "/";
const opt = (name, fallback) =>
  args.includes(`--${name}`) ? args[args.indexOf(`--${name}`) + 1] : fallback;
const clicks = args.flatMap((a, i) => (a === "--click" ? [args[i + 1]] : []));
const api = opt("api", "school") === "client" ? "http://127.0.0.1:47803" : "http://127.0.0.1:47802";
const user = opt("user", "admin");
const login = await (
  await fetch("http://127.0.0.1:47802/api/v1/auth/login", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      username: user,
      password: process.env.E2E_PASSWORD ?? "e2e-password-1",
    }),
  })
).json();
const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1366, height: 768 } });
await context.addInitScript(
  ({ token, api }) => {
    localStorage.setItem("madrasa.locale", "en");
    localStorage.setItem("madrasa.session", token);
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { windowLabel: "main", label: "main" },
      },
      transformCallback: () => 0,
      invoke: async (cmd) =>
        cmd === "api_base_url" ? api : cmd === "get_mode" ? { mode: "standalone" } : null,
    };
  },
  { token: login.token, api },
);
const page = await context.newPage();
await page.goto(`http://127.0.0.1:1425${route}`);
await page.waitForTimeout(1800);
for (const name of clicks) {
  const target = page
    .getByRole("button", { name, exact: true })
    .or(page.getByRole("tab", { name, exact: true }))
    .or(page.getByRole("link", { name, exact: true }));
  await target.first().click();
  await page.waitForTimeout(1200);
}
const seen = await page.evaluate(() => {
  const root =
    document.querySelector("[role=dialog]") ?? document.querySelector("main") ?? document.body;
  const text = (e) =>
    (e.getAttribute("aria-label") || e.innerText || "").replace(/\s+/g, " ").trim();
  const list = (sel) =>
    [...new Set([...root.querySelectorAll(sel)].map(text).filter(Boolean))].slice(0, 40);
  return {
    where: location.pathname + (document.querySelector("[role=dialog]") ? " (dialog)" : ""),
    headings: list("h1,h2,h3"),
    tabs: list("[role=tab]"),
    buttons: list("button"),
    links: list("a").slice(0, 15),
    fields: [...root.querySelectorAll("label")]
      .map((l) => l.innerText.replace(/\s+/g, " ").trim())
      .filter(Boolean)
      .slice(0, 40),
    columns: list("th"),
    rows: root.querySelectorAll("tbody tr").length,
    firstRow: root.querySelector("tbody tr")?.innerText.replace(/\s+/g, " ").slice(0, 160) ?? null,
    testids: [
      ...new Set(
        [...document.querySelectorAll("[data-testid]")].map((e) => e.getAttribute("data-testid")),
      ),
    ].slice(0, 40),
  };
});
console.log(JSON.stringify(seen, null, 1));
await browser.close();
