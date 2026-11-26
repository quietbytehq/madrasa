#!/usr/bin/env node
// How long each main screen takes to open on a large school (scripts/load-test.sh starts the
// server and the front end and runs this). A screen is "open" when the last request it made
// has been answered and nothing has been asked for 400 ms. The app is loaded once; each screen
// is then opened the way a click in the sidebar opens it, three times, and the slowest counts.
//
//   node scripts/load/screens.mjs <api> <front end> <password> [limit ms]
import { chromium } from "@playwright/test";

const [api, front, password, limitArg] = process.argv.slice(2);
const LIMIT = Number(limitArg ?? 1000);
const call = async (path, token, init = {}) => {
  const res = await fetch(`${api}/api/v1${path}`, {
    ...init,
    headers: { "content-type": "application/json", ...(token ? { authorization: `Bearer ${token}` } : {}) },
  });
  if (!res.ok) throw new Error(`${path}: ${res.status}`);
  return res.json();
};
const { token } = await call("/auth/login", null, {
  method: "POST",
  body: JSON.stringify({ username: "admin", password }),
});
const students = await call("/students?per_page=1", token);
const student = students.items[0];
const total = students.total;

// Background questions the app asks on a timer; not part of opening a screen.
const TICKING = [/\/health$/, /\/outbox/, /\/nexus\/status/, /\/backup\/status/, /\/offline/];

const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1366, height: 768 } });
await context.addInitScript(
  ({ token, api }) => {
    localStorage.setItem("madrasa.locale", "en");
    localStorage.setItem("madrasa.session", token);
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
      transformCallback: () => 0,
      invoke: async (cmd) =>
        cmd === "api_base_url" ? api : cmd === "get_mode" ? { mode: "standalone", server_url: null, fingerprint: null, listen_port: 8471 } : null,
    };
  },
  { token, api },
);
const page = await context.newPage();
let pending = 0;
let lastDone = 0;
let asked = [];
const mine = (url) => url.startsWith(`${api}/api/`) && !TICKING.some((t) => t.test(new URL(url).pathname));
page.on("request", (r) => {
  if (mine(r.url())) {
    pending += 1;
    asked.push({ url: r.url(), t: Date.now() });
  }
});
const done = (r) => {
  if (mine(r.url())) {
    pending -= 1;
    lastDone = Date.now();
    const a = asked.find((x) => x.url === r.url() && !x.ms);
    if (a) a.ms = lastDone - a.t;
  }
};
page.on("requestfinished", done);
page.on("requestfailed", done);

/** Wait until the screen has stopped asking; return ms from `t0` to the last answer. */
async function settled(t0) {
  let quietSince = null;
  for (;;) {
    await page.waitForTimeout(25);
    const now = Date.now();
    if (pending > 0) quietSince = null;
    else if (quietSince === null) quietSince = now;
    else if (now - quietSince >= 400) return Math.max(lastDone, t0) - t0;
    if (now - t0 > 60_000) throw new Error("the screen never settled");
  }
}

async function open(route, act) {
  asked = [];
  lastDone = 0;
  const t0 = Date.now();
  if (act) await act();
  else
    await page.evaluate((to) => {
      history.pushState({}, "", to);
      window.dispatchEvent(new PopStateEvent("popstate"));
    }, route);
  const ms = await settled(t0);
  const slowest = asked.filter((a) => a.ms).sort((a, b) => b.ms - a.ms)[0];
  return { ms, calls: asked.length, slowest: slowest ? `${new URL(slowest.url).pathname.replace("/api/v1", "")} ${slowest.ms} ms` : "" };
}

await page.goto(`${front}/`);
await settled(Date.now());

const tab = (name) => async () => {
  await page.getByRole("tab", { name, exact: true }).click();
};
// name, route to start from (null: stay where we are), and what to do there (default: open the route)
const SCREENS = [
  ["Dashboard", "/"],
  ["Students: list", "/students"],
  ["Students: search by name", null, async () => page.getByRole("searchbox", { name: /Search by name/ }).fill("Ali Khan")],
  ["Student: profile", `/students/${student.id}`],
  ["Student: fees tab", null, tab("Fees")],
  ["Student: attendance tab", null, tab("Attendance")],
  ["Student: results tab", null, tab("Results")],
  ["Attendance: mark", "/attendance"],
  ["Attendance: monthly register", null, tab("Monthly register")],
  ["Attendance: absentees", null, tab("Absentees")],
  ["Exams: marks entry", "/exams"],
  ["Exams: results", null, tab("Results")],
  ["Exams: report cards", null, tab("Report cards")],
  ["Fees: vouchers of the month", "/fees"],
  ["Fees: defaulters", null, tab("Defaulters")],
  ["Fees: daily collection", null, tab("Daily collection")],
  ["Fees: collect (a student's ledger)", null, async () => {
    await page.getByRole("tab", { name: "Collect", exact: true }).click();
    await page.getByLabel("Find a student by name or number").fill(student.student_number);
    await page.getByRole("button", { name: new RegExp(student.student_number) }).first().click();
  }],
  ["Teachers", "/teachers"],
  ["Reports", "/reports"],
  ["Import", "/import"],
  ["Data health", "/data-health"],
  ["Backup", "/backup"],
  ["Settings", "/settings"],
];

const results = [];
for (const [name, route, act] of SCREENS) {
  const runs = [];
  for (let i = 0; i < 3; i++) {
    if (act && i > 0) {
      // An action is measured from the screen it starts on: go away and come back.
      const back = page.url().replace(front, "");
      await open("/reports");
      await open(back);
      if (name.startsWith("Student:") || name.includes("register") || name.includes("absentees") || name.includes("Exams:") || name.includes("Fees:")) {
        // back on the first tab of that page
      }
    } else if (!act && i > 0) {
      await open("/reports");
    }
    try {
      runs.push(await open(route, act));
    } catch (e) {
      runs.push({ ms: Infinity, calls: 0, slowest: String(e.message).slice(0, 80) });
      break;
    }
  }
  const worst = runs.reduce((a, b) => (b.ms > a.ms ? b : a));
  results.push({ name, ms: worst.ms, best: Math.min(...runs.map((r) => r.ms)), calls: worst.calls, slowest: worst.slowest });
}
await browser.close();

console.log(`\nMain screens on a school of ${total} students (limit ${LIMIT} ms; slowest of three openings)\n`);
console.log("| Screen | Slowest | Fastest | Requests | Slowest request |");
console.log("| --- | ---: | ---: | ---: | --- |");
for (const r of results) {
  console.log(`| ${r.name} | ${r.ms} ms${r.ms > LIMIT ? " **over**" : ""} | ${r.best} ms | ${r.calls} | ${r.slowest} |`);
}
const over = results.filter((r) => r.ms > LIMIT);
console.log(over.length ? `\n${over.length} screen(s) over the limit.` : "\nEvery screen opened within the limit.");
process.exit(over.length ? 1 : 0);
