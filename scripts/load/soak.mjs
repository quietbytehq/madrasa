// Long-run test driver (scripts/soak-test.sh): a school server that is used for hours by
// people who come and go, while this script watches its memory and its database processes.
//
// What "use" means here, all through the real API:
//   - office staff sign in, open the dashboard, search students, open profiles and ledgers,
//     take fee payments, print vouchers, read reports; teachers mark attendance;
//   - half of them work through a client PC's proxy (as on a school network), and those
//     client PCs are switched off and on all the time, sometimes by pulling the plug;
//   - some people close the window in the middle of a download, some never sign out;
//   - a backup (with its restore check) and "Export everything" run every so often;
//   - the school server itself is restarted a few times, once by pulling the plug.
//
// What is checked:
//   - no request fails while the server it was sent to is up;
//   - the server's memory, open files and the number of database processes do not grow;
//   - after every stop of the server no database process is left behind (bundled
//     PostgreSQL only; with an outside database this cannot happen and is not checked);
//   - nothing is left waiting in a client's queue at the end.
//
// Environment: SOAK_BIN (madrasa-server), SOAK_WORK (folder), SOAK_MINUTES, SOAK_USERS,
// SOAK_DATABASE_URL (optional: an outside PostgreSQL instead of the bundled one).
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import path from "node:path";

const BIN = process.env.SOAK_BIN;
const WORK = process.env.SOAK_WORK;
const MINUTES = Number(process.env.SOAK_MINUTES ?? 480);
const USERS = Number(process.env.SOAK_USERS ?? 6);
const OUTSIDE_DB = process.env.SOAK_DATABASE_URL || "";
const PASSWORD = "soak-password-1";
const API_PORT = 47912;
const LAN_PORT = 47913;
const PROXY_PORTS = [47914, 47915];
const SCHOOL = `http://127.0.0.1:${API_PORT}`;
const SAMPLE_EVERY = MINUTES >= 30 ? 60_000 : 10_000;
// A change sent just as a machine goes down, or just after it comes back, may fail: that is
// the disconnection being simulated, not a fault.
const GRACE = 20_000;

if (!BIN || !WORK) throw new Error("SOAK_BIN and SOAK_WORK are needed");
mkdirSync(WORK, { recursive: true });
const dataDir = path.join(WORK, "school");
/** Write to a file of the run; a full disk must not end the run that is watching for trouble. */
const note = (file, text) => {
  try {
    appendFileSync(file, text);
  } catch {
    /* said on the console instead */
  }
};
const log = (line) => {
  const text = `${new Date().toISOString().slice(11, 19)} ${line}`;
  console.log(text);
  note(path.join(WORK, "soak.log"), text + "\n");
};
const pick = (list) => list[Math.floor(Math.random() * list.length)];
const chance = (p) => Math.random() < p;

// ---------------------------------------------------------------- processes

/** Every process this run started and that has not ended yet (none may be left at the end). */
const started = new Set();

/** A machine that can be up or down; requests sent to it while it was down do not count. */
class Machine {
  constructor(name) {
    this.name = name;
    this.child = null;
    this.up = false;
    this.changedAt = 0;
    this.starts = 0;
  }
  wasSteady(since) {
    return this.up && since - GRACE > this.changedAt;
  }
  mark(up) {
    this.up = up;
    this.changedAt = Date.now();
  }
  start(args, env, logName) {
    this.starts += 1;
    const child = spawn(BIN, args, {
      env: { ...process.env, RUST_LOG: "warn", ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const file = path.join(WORK, logName);
    child.stdout.on("data", (d) => note(file, d));
    child.stderr.on("data", (d) => note(file, d));
    this.child = child;
    started.add(child);
    this.exited = new Promise((resolve) =>
      child.once("exit", () => {
        started.delete(child);
        resolve();
      }),
    );
    return child;
  }
  async stop(signal) {
    if (!this.child) return;
    this.mark(false);
    this.child.kill(signal);
    await Promise.race([this.exited, sleep(60_000)]);
    this.child = null;
  }
}

async function healthy(base, tries = 240) {
  for (let i = 0; i < tries; i++) {
    try {
      const res = await fetch(`${base}/api/v1/health`, { signal: AbortSignal.timeout(2000) });
      if (res.ok) return true;
    } catch {
      /* not yet */
    }
    await sleep(500);
  }
  return false;
}

const server = new Machine("server");
async function startServer() {
  const env = { MADRASA_DATA_DIR: dataDir, MADRASA_BIND: `127.0.0.1:${API_PORT}`, MADRASA_LAN_BIND: `127.0.0.1:${LAN_PORT}` };
  if (OUTSIDE_DB) env.MADRASA_DATABASE_URL = OUTSIDE_DB;
  else delete process.env.MADRASA_DATABASE_URL;
  server.start([], env, "server.log");
  if (!(await healthy(SCHOOL))) throw new Error("the school server did not start (see server.log)");
  server.mark(true);
}

const proxies = PROXY_PORTS.map((port, i) => {
  const m = new Machine(`client ${i + 1}`);
  m.base = `http://127.0.0.1:${port}`;
  m.port = port;
  m.dir = path.join(WORK, `client${i + 1}`);
  return m;
});
async function startProxy(m) {
  // Never two of the same client: one that is still starting, or did not come up, goes first.
  if (m.child) await m.stop("SIGKILL");
  m.start(
    ["proxy"],
    {
      MADRASA_DATA_DIR: m.dir,
      MADRASA_SERVER_URL: `https://127.0.0.1:${LAN_PORT}`,
      MADRASA_SERVER_FINGERPRINT: readFileSync(path.join(dataDir, "tls", "fingerprint.txt"), "utf8").trim(),
      MADRASA_PROXY_BIND: `127.0.0.1:${m.port}`,
    },
    `${m.name.replace(" ", "")}.log`,
  );
  if (await healthy(m.base, 60)) m.mark(true);
}

/** Every process of the bundled database: the ones started for this run's data folder. */
function databaseProcesses() {
  const found = [];
  for (const pid of readdirSync("/proc")) {
    if (!/^\d+$/.test(pid)) continue;
    try {
      const cmd = readFileSync(`/proc/${pid}/cmdline`, "utf8").replace(/\0/g, " ");
      if (!/postgres/.test(cmd)) continue;
      if (cmd.includes(dataDir)) {
        found.push({ pid: Number(pid), postmaster: true });
        continue;
      }
      const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
      const ppid = Number(stat.slice(stat.lastIndexOf(")") + 2).split(" ")[1]);
      found.push({ pid: Number(pid), ppid });
    } catch {
      /* gone meanwhile */
    }
  }
  const masters = new Set(found.filter((p) => p.postmaster).map((p) => p.pid));
  return found.filter((p) => p.postmaster || masters.has(p.ppid));
}

function kb(pid, key) {
  try {
    const text = readFileSync(`/proc/${pid}/smaps_rollup`, "utf8");
    return Number(new RegExp(`^${key}:\\s+(\\d+)`, "m").exec(text)?.[1] ?? 0);
  } catch {
    return 0;
  }
}
const openFiles = (pid) => {
  try {
    return readdirSync(`/proc/${pid}/fd`).length;
  } catch {
    return 0;
  }
};

// ---------------------------------------------------------------- requests

const stats = { requests: 0, failures: [], kinds: new Map(), away: 0, queued: 0, dropped: 0, visits: 0 };
let running = true;

/** One request. Returns the parsed answer, or null when the machine it went to was away. */
async function call(base, machines, pathname, { method = "GET", body, token, raw = false, abortAfter } = {}) {
  const started = Date.now();
  stats.requests += 1;
  const controller = new AbortController();
  const limit = setTimeout(() => controller.abort(), abortAfter ?? 120_000);
  try {
    const res = await fetch(`${base}/api/v1${pathname}`, {
      method,
      signal: controller.signal,
      headers: {
        ...(body === undefined ? {} : { "content-type": "application/json" }),
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const bytes = Buffer.from(await res.arrayBuffer());
    if (res.ok) {
      if (raw) return bytes;
      const answer = bytes.length ? JSON.parse(bytes.toString("utf8")) : {};
      // 202 with "queued": the client PC kept the change to send when the server is back.
      if (res.status === 202 && answer?.queued) stats.queued += 1;
      return answer;
    }
    throw new Error(`${res.status} ${bytes.toString("utf8").slice(0, 200)}`);
  } catch (e) {
    if (abortAfter !== undefined && controller.signal.aborted) {
      stats.dropped += 1;
      return null;
    }
    if (!running || machines.some((m) => !m.wasSteady(started))) {
      stats.away += 1;
      return null;
    }
    stats.failures.push(`${new Date().toISOString()} ${method} ${pathname} via ${machines.map((m) => m.name).join("+")}: ${e.message}`);
    // Each kind of failure is logged the first few times; the count of each is in the result.
    const kind = `${method} ${pathname.replace(/[0-9a-f]{8}-[0-9a-f-]{27}/g, "{id}").replace(/\?.*/, "")}: ${e.message.slice(0, 120)}`;
    const seen = (stats.kinds.get(kind) ?? 0) + 1;
    stats.kinds.set(kind, seen);
    if (seen <= 3) log(`FAILED ${method} ${pathname}: ${e.message}`);
    return null;
  } finally {
    clearTimeout(limit);
  }
}

const scene = { logins: [], sections: [], today: new Date().toISOString().slice(0, 10) };

async function visit() {
  stats.visits += 1;
  const proxy = chance(0.5) ? pick(proxies) : null;
  const base = proxy ? proxy.base : SCHOOL;
  const machines = proxy ? [proxy, server] : [server];
  const ask = (p, o) => call(base, machines, p, o);
  const who = pick(scene.logins);
  const login = await ask("/auth/login", { method: "POST", body: { username: who.username, password: who.password } });
  if (!login?.token) return;
  const token = login.token;
  // Most people do a few things and go; one in ten stays at the desk for a minute, so some
  // are in the middle of their work when a machine goes down.
  const stays = chance(0.1);
  const steps = stays ? 40 : 3 + Math.floor(Math.random() * 6);
  for (let i = 0; i < steps && running; i++) {
    const r = Math.random();
    if (r < 0.15) await ask("/dashboard", { token });
    else if (r < 0.35) {
      const list = await ask(`/students?page_size=25&q=${pick(["a", "an", "sh", "ali", "kh", "m"])}`, { token });
      const s = list?.items?.length ? pick(list.items) : null;
      if (s) {
        await ask(`/students/${s.id}`, { token });
        await ask(`/fees/students/${s.id}/ledger`, { token });
      }
    } else if (r < 0.55) {
      // A fee payment of a small part of a voucher, then its receipt and the printed voucher.
      const list = await ask("/fees/vouchers?status=unpaid&limit=50", { token });
      const open = (Array.isArray(list) ? list : []).filter((v) => Number(v.total_due) + Number(v.late_fee) - Number(v.net_paid) >= 50);
      if (open.length) {
        const v = pick(open);
        const paid = await ask(`/fees/vouchers/${v.id}/payments`, { method: "POST", token, body: { amount: "1.00", method: "cash" } });
        if (paid?.receipt?.id) await ask(`/fees/receipts/${paid.receipt.id}`, { token });
        await ask(`/fees/vouchers/${v.id}/pdf`, { token, raw: true });
      }
    } else if (r < 0.75) {
      // Attendance for a section: read the sheet, save it with one or two absentees.
      const section = pick(scene.sections);
      const sheet = await ask(`/attendance/sheet?section_id=${section.id}&date=${scene.day}`, { token });
      if (sheet?.can_edit && sheet.students?.length) {
        const entries = sheet.students.filter(() => chance(0.06)).map((s) => ({ student_id: s.student_id, status: pick(["absent", "late", "leave"]) }));
        await ask("/attendance/sheet", { method: "PUT", token, body: { section_id: section.id, date: scene.day, entries } });
      }
    } else if (r < 0.9) {
      await ask(pick(["/fees/defaulters", "/fees/collections", `/attendance/absentees?date=${scene.day}`, "/data-health", "/backup/status"]), { token });
    } else {
      // Someone closes the window in the middle of a download: a batch of vouchers to
      // print, or the whole school as a workbook.
      const list = chance(0.5) ? await ask("/fees/vouchers?limit=20", { token }) : null;
      const ids = (Array.isArray(list) ? list : []).map((v) => v.id).join(",");
      await ask(ids ? `/fees/vouchers/pdf?ids=${ids}` : "/export/everything", { token, raw: true, abortAfter: 5 + Math.random() * 150 });
    }
    await sleep(stays ? 500 + Math.random() * 1500 : 50 + Math.random() * 250);
  }
  // Half sign out; the others just leave.
  if (chance(0.5)) await ask("/auth/logout", { method: "POST", token, body: {} });
}

async function worker() {
  while (running) {
    await visit();
    await sleep(100 + Math.random() * 400);
  }
}

/** Client PCs are switched off and on, one after the other, sometimes by pulling the plug. */
async function clientsComeAndGo() {
  let n = 0;
  while (running) {
    await sleep(20_000 + Math.random() * 40_000);
    if (!running) break;
    const m = proxies[n % proxies.length];
    n += 1;
    await m.stop(chance(0.5) ? "SIGTERM" : "SIGKILL");
    await sleep(2_000 + Math.random() * 8_000);
    if (running && server.up) await startProxy(m);
  }
}

/** Now and then: a backup with its check, and "Export everything" to the end. */
async function housekeeping(adminToken) {
  const every = Math.max(60_000, Math.min(15 * 60_000, (MINUTES * 60_000) / 12));
  while (running) {
    await sleep(every);
    if (!running || !server.up) continue;
    const token = await adminToken();
    if (!token) continue;
    const run = await call(SCHOOL, [server], "/backup/run", { method: "POST", token, body: {} });
    if (run) housekept.backups += 1;
    const book = await call(SCHOOL, [server], "/export/everything", { token, raw: true });
    if (book) housekept.exports += 1;
  }
}
const housekept = { backups: 0, exports: 0, checks: {} };

// ---------------------------------------------------------------- watching

const samples = [];
const csv = path.join(WORK, "samples.csv");
function sample() {
  if (!server.up || !server.child) return;
  const pid = server.child.pid;
  const db = OUTSIDE_DB ? [] : databaseProcesses();
  const row = {
    t: Date.now(),
    uptime: server.starts,
    serverKb: kb(pid, "Pss"),
    files: openFiles(pid),
    dbProcesses: db.length,
    dbKb: db.reduce((sum, p) => sum + kb(p.pid, "Pss"), 0),
    requests: stats.requests,
    failures: stats.failures.length,
  };
  samples.push(row);
  note(csv, `${new Date(row.t).toISOString()},${row.uptime},${row.serverKb},${row.files},${row.dbProcesses},${row.dbKb},${row.requests},${row.failures}\n`);
}

const stops = [];
/** Stop the school server and count what it left behind; start it again unless `last`. */
async function restartServer(signal, last = false) {
  log(`school server: ${signal === "SIGKILL" ? "power cut" : "stopped"}${last ? " (end of the run)" : ""}`);
  await server.stop(signal);
  let left = [];
  if (!OUTSIDE_DB) {
    // A clean stop takes the database down with it. After a power cut the database is
    // orphaned by definition; what matters then is that the next start ends up with one.
    for (let i = 0; i < 60; i++) {
      left = databaseProcesses();
      if (left.length === 0 || signal === "SIGKILL") break;
      await sleep(500);
    }
  }
  const record = { at: new Date().toISOString(), signal, left: left.length, after: null };
  stops.push(record);
  if (last) return;
  await startServer();
  if (!OUTSIDE_DB) {
    await sleep(3000);
    record.after = databaseProcesses().filter((p) => p.postmaster).length;
  }
  for (const m of proxies) if (!m.up && !m.child) await startProxy(m);
}

// ---------------------------------------------------------------- the run

function slopePerHour(rows, key) {
  if (rows.length < 5) return null;
  const t0 = rows[0].t;
  const xs = rows.map((r) => (r.t - t0) / 3_600_000);
  const ys = rows.map((r) => r[key]);
  const mx = xs.reduce((a, b) => a + b, 0) / xs.length;
  const my = ys.reduce((a, b) => a + b, 0) / ys.length;
  let num = 0;
  let den = 0;
  for (let i = 0; i < xs.length; i++) {
    num += (xs[i] - mx) * (ys[i] - my);
    den += (xs[i] - mx) ** 2;
  }
  return den === 0 ? 0 : num / den;
}
const median = (list) => [...list].sort((a, b) => a - b)[Math.floor(list.length / 2)] ?? 0;

async function main() {
  writeFileSync(csv, "time,uptime,server_pss_kb,server_open_files,db_processes,db_pss_kb,requests,failures\n");
  log(`long-run test: ${MINUTES} minutes, ${USERS} people at a time, ${OUTSIDE_DB ? "an outside database" : "the bundled database"}`);
  await startServer();
  const setup = await fetch(`${SCHOOL}/api/v1/setup/demo`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ admin_username: "admin", admin_password: PASSWORD, backup_password: "soak-backup-password-1", size: "small" }),
  });
  if (!setup.ok) throw new Error(`demo school: ${setup.status} ${await setup.text()}`);
  const adminToken = async () => (await call(SCHOOL, [server], "/auth/login", { method: "POST", body: { username: "admin", password: PASSWORD } }))?.token;
  const token = await adminToken();
  scene.logins.push({ username: "admin", password: PASSWORD });
  scene.sections = await call(SCHOOL, [server], "/sections", { token });
  // The last day the school was open, so attendance can be saved whatever day this runs on.
  for (let back = 0; back < 10; back++) {
    const day = new Date(Date.now() - back * 86_400_000).toISOString().slice(0, 10);
    const sheet = await call(SCHOOL, [server], `/attendance/sheet?section_id=${scene.sections[0].id}&date=${day}`, { token });
    if (sheet?.can_edit) {
      scene.day = day;
      break;
    }
  }
  if (!scene.day) throw new Error("no school day in the last ten days to mark attendance on");
  const backups = path.join(WORK, "backups");
  mkdirSync(backups, { recursive: true });
  await call(SCHOOL, [server], "/backup/settings", { method: "PUT", token, body: { enabled: true, time: "02:00", directory: backups, keep: 3 } });
  for (const m of proxies) await startProxy(m);

  const end = Date.now() + MINUTES * 60_000;
  const sampler = setInterval(sample, SAMPLE_EVERY);
  const background = [clientsComeAndGo(), housekeeping(adminToken), ...Array.from({ length: USERS }, worker)];

  // The school server is restarted three times, evenly spread; the second is a power cut.
  const plan = [0.25, 0.5, 0.75].map((f, i) => ({ at: Date.now() + f * MINUTES * 60_000, signal: i === 1 ? "SIGKILL" : "SIGTERM" }));
  let lastNote = Date.now();
  while (Date.now() < end) {
    await sleep(1000);
    if (plan.length && Date.now() >= plan[0].at) await restartServer(plan.shift().signal);
    if (Date.now() - lastNote > Math.max(60_000, (MINUTES * 60_000) / 48)) {
      lastNote = Date.now();
      const s = samples.at(-1);
      log(`${Math.round((end - Date.now()) / 60_000)} min left: ${stats.requests} requests, ${stats.failures.length} failed, server ${s ? Math.round(s.serverKb / 1024) : "?"} MB, ${s?.dbProcesses ?? "?"} database processes`);
    }
  }

  // The day ends: people stop, the clients send what they still hold, everything is shut.
  running = false;
  clearInterval(sampler);
  await Promise.allSettled(background);
  for (const m of proxies) if (!m.up) await startProxy(m);
  const waiting = [];
  for (const m of proxies) {
    let left = null;
    for (let i = 0; i < 120; i++) {
      try {
        left = (await (await fetch(`${m.base}/__madrasa/outbox/status`)).json()).waiting;
      } catch {
        left = null;
      }
      if (left === 0) break;
      await sleep(1000);
    }
    waiting.push(left);
  }
  const final = await adminToken();
  const status = final ? await call(SCHOOL, [server], "/backup/status", { token: final }) : null;
  for (const m of proxies) await m.stop("SIGTERM");
  await restartServer("SIGTERM", true);
  // Nothing this run started may still be running.
  await sleep(1000);
  const leftover = [...started].map((c) => c.pid);
  for (const c of started) c.kill("SIGKILL");

  // ------------------------------------------------------------ the verdict
  const problems = [];
  const lines = [];
  lines.push(`# Long-run test`, "");
  lines.push(`- Ran for ${MINUTES} minutes with ${USERS} people at a time, half of them through ${proxies.length} client PCs; ${OUTSIDE_DB ? "outside PostgreSQL" : "bundled PostgreSQL"}.`);
  lines.push(`- ${stats.visits} visits, ${stats.requests} requests. ${stats.away} were sent to a machine that was switched off at that moment, ${stats.dropped} were closed by the person half-way, ${stats.queued} changes were kept by a client and sent later.`);
  lines.push(`- Client PCs were switched off and on ${proxies.reduce((n, m) => n + m.starts - 1, 0)} times. The school server was restarted ${stops.length - 1} times (one power cut).`);
  lines.push(`- Backups made and checked: ${housekept.backups}; "Export everything" downloaded: ${housekept.exports}. Last backup check: ${status?.check ?? "none"}.`);
  lines.push("");
  lines.push(`**Requests that failed while every machine involved was up: ${stats.failures.length}**`);
  if (stats.failures.length) problems.push(`${stats.failures.length} requests failed`);
  for (const [kind, n] of [...stats.kinds].sort((a, b) => b[1] - a[1]).slice(0, 15)) lines.push(`  - ${n} × ${kind}`);
  if (stats.failures.length) writeFileSync(path.join(WORK, "failures.txt"), stats.failures.join("\n") + "\n");

  // Memory and open files: the longest stretch between two restarts, after its warm-up.
  const byUptime = new Map();
  for (const s of samples) byUptime.set(s.uptime, [...(byUptime.get(s.uptime) ?? []), s]);
  const longest = [...byUptime.values()].sort((a, b) => b.length - a.length)[0] ?? [];
  const steady = longest.slice(Math.ceil(longest.length * 0.2));
  const hours = steady.length ? (steady.at(-1).t - steady[0].t) / 3_600_000 : 0;
  lines.push("", `## Memory, open files, database processes`, "");
  lines.push(`Judged on the longest stretch between two restarts (${hours.toFixed(2)} h after warm-up, ${steady.length} samples).`, "");
  lines.push("| | first | median | last | trend per hour |", "|---|---|---|---|---|");
  const judge = (key, label, unit, scale, allowedShare, allowedAbs) => {
    if (!steady.length) return;
    const values = steady.map((s) => s[key]);
    const slope = slopePerHour(steady, key);
    const mid = median(values);
    const fmt = (v) => `${(v / scale).toFixed(scale === 1 ? 0 : 1)} ${unit}`;
    // A trend over a few minutes says nothing (the first backup alone doubles the memory).
    const trend = slope === null || MINUTES < 30 ? "too short to say" : `${slope >= 0 ? "+" : ""}${fmt(slope)}`;
    lines.push(`| ${label} | ${fmt(values[0])} | ${fmt(mid)} | ${fmt(values.at(-1))} | ${trend} |`);
    if (slope === null || allowedShare === null) return;
    const growth = slope * hours;
    if (MINUTES >= 30 && growth > Math.max(mid * allowedShare, allowedAbs)) problems.push(`${label} grew by about ${fmt(growth)} over ${hours.toFixed(1)} h`);
  };
  judge("serverKb", "School server memory", "MB", 1024, 0.2, 30 * 1024);
  judge("files", "School server open files", "", 1, 0.2, 20);
  if (!OUTSIDE_DB) {
    judge("dbProcesses", "Database processes", "", 1, 0.2, 4);
    judge("dbKb", "Database memory", "MB", 1024, null, null);
  }
  if (MINUTES < 30) lines.push("", "_The run was shorter than 30 minutes: growth is not judged._");

  lines.push("", `## Stops of the school server`, "");
  for (const s of stops) {
    if (OUTSIDE_DB) lines.push(`- ${s.at} ${s.signal}: outside database, nothing to leave behind (not checked).`);
    else if (s.signal === "SIGKILL") {
      lines.push(`- ${s.at} power cut: ${s.left} database processes orphaned (expected); after the next start there was ${s.after} database server.`);
      if (s.after !== 1) problems.push(`after a power cut the next start ended with ${s.after} database servers`);
    } else {
      lines.push(`- ${s.at} stopped: ${s.left} database processes left behind${s.after === null ? "" : `; after the next start: ${s.after} database server`}.`);
      if (s.left !== 0) problems.push(`${s.left} database processes were left behind after a stop`);
      if (s.after !== null && s.after !== 1) problems.push(`${s.after} database servers after a restart`);
    }
  }
  lines.push("", `## Client queues at the end`, "");
  waiting.forEach((w, i) => {
    lines.push(`- client ${i + 1}: ${w === null ? "did not answer" : `${w} waiting`}`);
    if (w !== 0) problems.push(`client ${i + 1} still had ${w ?? "an unknown number of"} changes waiting`);
  });
  if (leftover.length) problems.push(`${leftover.length} server or client processes did not stop when told to (pids ${leftover.join(", ")})`);
  if (status && status.check === "failed") problems.push(`the last backup failed its check: ${status.check_message}`);
  if (housekept.backups === 0 && MINUTES >= 5) problems.push("no backup was made during the run");

  lines.push("", problems.length ? `## Result: FAILED\n\n${problems.map((p) => `- ${p}`).join("\n")}` : "## Result: passed");
  writeFileSync(path.join(WORK, "results.md"), lines.join("\n") + "\n");
  console.log("\n" + lines.join("\n"));
  process.exit(problems.length ? 1 : 0);
}

const bail = async (e) => {
  log(`stopped: ${e?.stack ?? e}`);
  running = false;
  for (const c of started) c.kill("SIGTERM");
  await sleep(3000);
  for (const c of started) c.kill("SIGKILL");
  process.exit(2);
};
process.on("uncaughtException", bail);
process.on("SIGINT", () => bail("interrupted"));
process.on("SIGTERM", () => bail("interrupted"));
main().catch(bail);
