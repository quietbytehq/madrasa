// The scenes that make the manual's screenshots, in the order they run (scenes that change
// the demo data in ways others would notice run last). Each scene lists the screenshot ids it
// writes; docs/manual/manual.yaml uses the same ids. Labels are looked up by translation key
// (s.L), so every scene works in English and Urdu alike.

import { URLS, PASSWORD, SETUP_TOKEN, settle } from "./shoot.mjs";

// ---- small helpers -------------------------------------------------------------------------
const by = (s, page) => ({
  button: (key, params) => page.getByRole("button", { name: s.L(key, params), exact: true }),
  tab: (key) => page.getByRole("tab", { name: s.L(key), exact: true }).or(page.getByRole("button", { name: s.L(key), exact: true })),
  link: (key) => page.getByRole("link", { name: s.L(key), exact: true }),
  field: (key) => page.getByLabel(s.L(key), { exact: true }),
  heading: (key) => page.getByRole("heading", { name: s.L(key), exact: true }),
  card: (key) => page.getByRole("heading", { name: s.L(key), exact: true }).first().locator("xpath=ancestor::section[1]"),
  dialog: () => page.getByRole("dialog").last(),
  text: (key, params) => page.getByText(s.L(key, params), { exact: true }),
});

async function click(locator) {
  await locator.first().click();
  await locator.first().page().waitForTimeout(250);
}

const first = (locator) => locator.first();

export const scenes = [];
const scene = (name, shots, run) => scenes.push({ name, shots, run });

// ---- Part 1: getting started ---------------------------------------------------------------

scene("first start: how this PC is used", ["install-mode"], async (s) => {
  const page = await s.desktop({ mode: null, signedIn: false });
  await s.snap(page, "install-mode");
  await page.context().close();
});

scene("setup wizard", ["setup-school", "setup-year", "setup-admin", "setup-backup", "setup-done", "login"], async (s) => {
  const page = await s.desktop({ api: URLS.setup, signedIn: false });
  const $ = by(s, page);
  await $.field("setup.school.name").fill("DEMO Iqra Model School");
  await $.field("setup.school.code").fill("DEMO02");
  await page.getByLabel(s.L("setup.school.city"), { exact: false }).first().fill("Lahore");
  await s.snap(page, "setup-school");
  await click($.button("action.continue"));
  await s.snap(page, "setup-year");
  await click($.button("action.continue"));
  await $.field("setup.admin.fullName").fill("DEMO Principal");
  await $.field("setup.admin.username").fill("principal");
  await $.field("setup.admin.password").fill(PASSWORD);
  await $.field("setup.admin.passwordConfirm").fill(PASSWORD);
  await s.snap(page, "setup-admin");
  await click($.button("action.continue"));
  await $.field("setup.backup.password").fill("demo-backup-password-1");
  await $.field("setup.backup.passwordConfirm").fill("demo-backup-password-1");
  await s.snap(page, "setup-backup");
  await click($.button("action.finish"));
  await page.getByText(s.L("setup.done.title")).first().waitFor({ timeout: 30000 });
  await s.snap(page, "setup-done");
  await page.getByLabel(s.L("setup.done.ack")).first().check().catch(() => {});
  await page.goto(`${URLS.desktop}/login`);
  await settle(page);
  await page.getByLabel(s.L("login.username")).fill("principal");
  await s.snap(page, "login");
  await page.context().close();
});

scene("the screen at a glance", ["overview-dashboard", "overview-usermenu", "overview-help"], async (s) => {
  const page = await s.desktop();
  const $ = by(s, page);
  await s.snap(page, "overview-dashboard");
  await click($.button("topbar.userMenu"));
  await s.snap(page, "overview-usermenu");
  await page.keyboard.press("Escape");
  await click($.button("topbar.help"));
  await s.snap(page, "overview-help");
  await page.context().close();
});

// ---- Part 2: the school's daily work -------------------------------------------------------

scene("settings", ["settings-profile", "settings-school", "settings-users"], async (s) => {
  const page = await s.desktop({ route: "/settings" });
  const $ = by(s, page);
  await s.snap(page, "settings-profile", { target: $.card("settings.profile") });
  await s.snap(page, "settings-school", { target: $.card("settings.school") });
  await $.card("users.title").scrollIntoViewIfNeeded();
  await s.snap(page, "settings-users", { target: $.card("users.title") });
  await page.context().close();
});

scene("academic structure", ["acad-years", "acad-year-new", "acad-classes", "acad-sections", "acad-section-new", "acad-subjects", "acad-groups", "acad-consents"], async (s) => {
  const page = await s.desktop({ route: "/settings" });
  const $ = by(s, page);
  await s.snap(page, "acad-years", { target: $.card("academics.years") });
  await click($.button("academics.years.new"));
  await s.snap(page, "acad-year-new", { target: $.dialog() });
  await page.keyboard.press("Escape");
  await s.snap(page, "acad-classes", { target: $.card("academics.classes") });
  await s.snap(page, "acad-sections", { target: $.card("academics.sections") });
  await click($.button("academics.sections.new"));
  await s.snap(page, "acad-section-new", { target: $.dialog() });
  await page.keyboard.press("Escape");
  await s.snap(page, "acad-subjects", { target: $.card("academics.subjects") });
  await s.snap(page, "acad-groups", { target: $.card("academics.groups") });
  await s.snap(page, "acad-consents", { target: $.card("consents.types") });
  await page.context().close();
});

scene("staff", ["staff-list", "staff-add", "staff-profile", "staff-allocation"], async (s) => {
  const page = await s.desktop({ route: "/teachers" });
  const $ = by(s, page);
  await s.snap(page, "staff-list");
  await click($.button("staff.add"));
  await s.snap(page, "staff-add");
  await page.keyboard.press("Escape");
  await page.goto(`${URLS.desktop}/teachers`);
  await settle(page);
  await click(page.locator("tbody tr").first());
  await settle(page, 600);
  await s.snap(page, "staff-profile");
  await page.goto(`${URLS.desktop}/teachers/allocation`);
  await settle(page, 800);
  await s.snap(page, "staff-allocation");
  await page.context().close();
});

/** The first student of a section (by the API), for profile screens. */
async function aStudent(s) {
  const token = await s.token("demo.khadija.iqbal");
  const res = await fetch(`${URLS.school}/api/v1/students?status=active&page=1&per_page=1`, {
    headers: { authorization: `Bearer ${token}` },
  });
  const body = await res.json();
  const items = Array.isArray(body) ? body : (body.items ?? body.rows ?? []);
  return items[0];
}

scene("students", ["students-list", "students-add", "students-profile", "students-profile-fees", "students-withdraw", "students-subjects", "students-moves", "students-consents"], async (s) => {
  const page = await s.desktop({ route: "/students" });
  const $ = by(s, page);
  await settle(page, 600);
  await s.snap(page, "students-list");
  await click($.button("students.add"));
  await settle(page, 500);
  await s.snap(page, "students-add");
  const student = await aStudent(s);
  await page.goto(`${URLS.desktop}/students/${student.id}`);
  await settle(page, 700);
  await s.snap(page, "students-profile");
  await click($.tab("students.tabs.fees"));
  await settle(page, 600);
  await s.snap(page, "students-profile-fees");
  await click($.button("students.withdraw"));
  await s.snap(page, "students-withdraw", { target: $.dialog() });
  await page.keyboard.press("Escape");
  await page.goto(`${URLS.desktop}/students/subjects`);
  await settle(page, 500);
  // A section of class 9, where students choose electives.
  const section = page.getByLabel(s.L("common.section"), { exact: true });
  if (await section.count()) {
    const value = await section.locator("option").evaluateAll((opts) => (opts.find((o) => /^9/.test(o.textContent.trim())) ?? opts[1] ?? opts[0])?.value);
    if (value) await section.selectOption(value);
  }
  await settle(page, 700);
  await s.snap(page, "students-subjects");
  await page.goto(`${URLS.desktop}/students/sections`);
  await settle(page, 500);
  const cls = page.getByLabel(s.L("common.class"), { exact: true });
  if (await cls.count()) {
    const value = await cls.locator("option").evaluateAll((opts) => opts.find((o) => o.textContent.trim() === "5")?.value ?? opts[1]?.value);
    if (value) await cls.selectOption(value);
  }
  await settle(page, 700);
  await s.snap(page, "students-moves");
  await page.goto(`${URLS.desktop}/students/consents`);
  await settle(page, 600);
  const consentSection = page.getByLabel(s.L("common.section"), { exact: true });
  const firstSection = await consentSection.locator("option").evaluateAll((opts) => opts.find((o) => o.value)?.value);
  if (firstSection) await consentSection.selectOption(firstSection);
  await settle(page, 900);
  await s.snap(page, "students-consents");
  await page.context().close();
});

scene("import", ["import-start", "import-students"], async (s) => {
  const page = await s.desktop({ route: "/import" });
  await s.snap(page, "import-start");
  await click(page.getByRole("button", { name: new RegExp(`^${s.L("import.kind.students")}`) }));
  await settle(page, 500);
  await s.snap(page, "import-students");
  await page.context().close();
});

scene("attendance", ["att-mark", "att-submitted", "att-absentees", "att-register", "att-staff", "att-calendar"], async (s) => {
  const page = await s.desktop({ route: "/attendance" });
  const $ = by(s, page);
  await settle(page, 700);
  // A school day: the last Monday.
  await page.getByLabel(s.L("attendance.date"), { exact: true }).fill(lastMonday()).catch(() => {});
  await settle(page, 900);
  // Mark two students: one absent, one late.
  await page.getByRole("radio", { name: s.L("attendance.status.absent"), exact: true }).nth(1).click();
  await page.getByRole("radio", { name: s.L("attendance.status.late"), exact: true }).nth(3).click();
  await s.snap(page, "att-mark");
  await click($.button("attendance.mark.submit"));
  await settle(page, 700);
  await s.snap(page, "att-submitted");
  for (const [tab, id] of [
    ["attendance.tabs.absentees", "att-absentees"],
    ["attendance.tabs.register", "att-register"],
    ["attendance.tabs.staff", "att-staff"],
    ["attendance.tabs.calendar", "att-calendar"],
  ]) {
    await click($.tab(tab));
    await settle(page, 700);
    await s.snap(page, id);
  }
  await page.context().close();
});

scene("fees", ["fees-setup", "fees-generate", "fees-vouchers", "fees-voucher-print", "fees-collect", "fees-pay", "fees-receipt", "fees-defaulters", "fees-daily"], async (s) => {
  const page = await s.desktop({ route: "/fees" });
  const $ = by(s, page);
  await click($.tab("fees.tabs.setup"));
  await settle(page, 700);
  await s.snap(page, "fees-setup");
  await click($.tab("fees.tabs.vouchers"));
  await click($.button("fees.generate"));
  await s.snap(page, "fees-generate", { target: $.dialog() });
  await click($.dialog().getByRole("button", { name: s.L("fees.generate"), exact: true }));
  await page.getByRole("dialog").waitFor({ state: "detached", timeout: 120000 }).catch(() => {});
  await settle(page, 1500);
  await s.snap(page, "fees-vouchers");
  // A voucher of this month, printed.
  const token = await s.token("demo.khadija.iqbal");
  const month = new Date().toISOString().slice(0, 7);
  const list = await (await fetch(`${URLS.school}/api/v1/fees/vouchers?month=${month}-01&per_page=5`, { headers: { authorization: `Bearer ${token}` } })).json();
  const vouchers = Array.isArray(list) ? list : (list.items ?? list.rows ?? []);
  const voucher = vouchers[0];
  await page.goto(`${URLS.desktop}/fees/vouchers/${voucher.id}/print`);
  await settle(page, 900);
  await s.snap(page, "fees-voucher-print");
  // Collect: find the student, record the payment, print the receipt.
  await page.goto(`${URLS.desktop}/fees`);
  await settle(page);
  await click($.tab("fees.tabs.collect"));
  await page.getByLabel(s.L("fees.student.search"), { exact: true }).fill(voucher.student_number);
  await settle(page, 900);
  await click(page.getByText(voucher.student_name, { exact: true }));
  await settle(page, 900);
  await s.snap(page, "fees-collect");
  await click(page.getByRole("button", { name: s.L("fees.pay"), exact: true }));
  await s.snap(page, "fees-pay", { target: $.dialog() });
  await click($.dialog().getByRole("button", { name: s.L("fees.pay"), exact: true }));
  await settle(page, 1200);
  const receipts = await (await fetch(`${URLS.school}/api/v1/fees/vouchers/${voucher.id}`, { headers: { authorization: `Bearer ${token}` } })).json();
  const receipt = receipts.receipts?.[0];
  if (receipt) {
    await page.goto(`${URLS.desktop}/fees/receipts/${receipt.id}/print`);
    await settle(page, 900);
  }
  await s.snap(page, "fees-receipt");
  await page.goto(`${URLS.desktop}/fees`);
  await settle(page);
  await click($.tab("fees.tabs.defaulters"));
  // As at the end of the month, when this month's vouchers are overdue.
  const now = new Date();
  const end = new Date(Date.UTC(now.getFullYear(), now.getMonth() + 1, 0)).toISOString().slice(0, 10);
  await page.locator('input[type="date"]').first().fill(end);
  await settle(page, 1500);
  await s.snap(page, "fees-defaulters");
  await click($.tab("fees.tabs.collections"));
  await settle(page, 900);
  await s.snap(page, "fees-daily");
  await page.context().close();
});

scene("exams", ["exams-setup", "exams-marks", "exams-sheet", "exams-results", "exams-cards", "exams-boards"], async (s) => {
  const page = await s.desktop({ route: "/exams" });
  const $ = by(s, page);
  await click($.tab("exams.tabs.setup"));
  await settle(page, 800);
  await s.snap(page, "exams-setup");
  await click($.tab("exams.tabs.marks"));
  await settle(page, 800);
  await s.snap(page, "exams-marks");
  await click($.button("exams.sheets.open"));
  await settle(page, 1000);
  await s.snap(page, "exams-sheet");
  await page.goto(`${URLS.desktop}/exams`);
  await settle(page);
  for (const [tab, id] of [
    ["exams.tabs.results", "exams-results"],
    ["exams.tabs.reportCards", "exams-cards"],
    ["exams.tabs.boards", "exams-boards"],
  ]) {
    await click($.tab(tab));
    await settle(page, 1200);
    await s.snap(page, id);
  }
  await page.context().close();
});

scene("reports", ["reports-list", "reports-run", "reports-custom"], async (s) => {
  const page = await s.desktop({ route: "/reports" });
  const $ = by(s, page);
  await settle(page, 800);
  await s.snap(page, "reports-list");
  await page.goto(`${URLS.desktop}/reports/students.strength`);
  await settle(page, 800);
  await click($.button("reports.run"));
  await settle(page, 1500);
  await s.snap(page, "reports-run");
  await page.goto(`${URLS.desktop}/reports/custom/new`);
  await settle(page, 900);
  await s.snap(page, "reports-custom");
  await page.context().close();
});

scene("backup", ["backup-page", "backup-restore"], async (s) => {
  const page = await s.desktop({ route: "/backup" });
  const $ = by(s, page);
  await settle(page, 700);
  await s.snap(page, "backup-page");
  await s.snap(page, "backup-restore", { target: page.getByLabel(s.L("backup.files.otherPath"), { exact: true }).locator("xpath=ancestor::section[1]") });
  await page.context().close();
});

// ---- Part 3: the Standard edition ----------------------------------------------------------

/** Open `route`, then each tab in turn, saving one screenshot per tab. */
async function tabs(s, route, list, { first = null } = {}) {
  const page = await s.desktop({ route });
  const $ = by(s, page);
  await settle(page, 900);
  if (first) await s.snap(page, first);
  for (const [tab, id] of list) {
    await click($.tab(tab));
    await settle(page, 1000);
    await s.snap(page, id);
  }
  return page;
}

scene("edition", ["edition-card"], async (s) => {
  const page = await s.desktop({ route: "/settings" });
  const $ = by(s, page);
  await s.snap(page, "edition-card", { target: $.card("settings.edition") });
  await page.context().close();
});

/** The last Monday (a school day), as yyyy-mm-dd. */
function lastMonday() {
  const d = new Date();
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7 || 7));
  return d.toISOString().slice(0, 10);
}

scene("timetable", ["tt-setup", "tt-lessons", "tt-generate", "tt-view", "tt-subs"], async (s) => {
  const page = await tabs(s, "/timetable", [
    ["tt.tab.setup", "tt-setup"],
    ["tt.tab.lessons", "tt-lessons"],
    ["tt.tab.generate", "tt-generate"],
  ]);
  const $ = by(s, page);
  await click($.tab("tt.tab.view"));
  await settle(page, 900);
  // The first section the demo timetable fills.
  const section = page.getByLabel(s.L("common.section"), { exact: true });
  const values = await section.locator("option").evaluateAll((opts) => opts.map((o) => o.value));
  let best = [values[0], 0];
  for (const v of values) {
    await section.selectOption(v);
    await page.waitForTimeout(250);
    const filled = await page.getByTestId("timetable-grid").evaluate((g) => g.innerText.length);
    if (filled > best[1]) best = [v, filled];
  }
  await section.selectOption(best[0]);
  await s.snap(page, "tt-view");
  await click($.tab("tt.tab.substitutions"));
  await settle(page, 800);
  await page.locator('input[type="date"]').first().fill(lastMonday());
  await settle(page, 900);
  await s.snap(page, "tt-subs");
  await page.context().close();
});

scene("leave", ["leave-types", "leave-staff", "leave-request", "leave-balances", "leave-students", "leave-register"], async (s) => {
  const page = await s.desktop({ route: "/leave" });
  const $ = by(s, page);
  await click($.tab("leave.tab.types"));
  await settle(page, 800);
  await s.snap(page, "leave-types");
  await click($.tab("leave.tab.requests"));
  await settle(page, 800);
  await s.snap(page, "leave-staff");
  await click($.button("leave.requests.newFor"));
  await s.snap(page, "leave-request", { target: $.dialog() });
  await page.keyboard.press("Escape");
  for (const [tab, id] of [
    ["leave.tab.balances", "leave-balances"],
    ["leave.tab.students", "leave-students"],
    ["leave.tab.register", "leave-register"],
  ]) {
    await click($.tab(tab));
    await settle(page, 900);
    await s.snap(page, id);
  }
  await page.context().close();
});

scene("admissions", ["adm-stages", "adm-board", "adm-new", "adm-list", "adm-detail"], async (s) => {
  const page = await s.desktop({ route: "/admissions" });
  const $ = by(s, page);
  await click($.tab("adm.tab.stages"));
  await settle(page, 800);
  await s.snap(page, "adm-stages");
  await click($.tab("adm.tab.board"));
  await settle(page, 800);
  await s.snap(page, "adm-board");
  await click($.button("adm.new"));
  await s.snap(page, "adm-new", { target: $.dialog() });
  await page.keyboard.press("Escape");
  await click($.tab("adm.tab.list"));
  await settle(page, 800);
  await s.snap(page, "adm-list");
  await click(page.locator("tbody tr").first());
  await settle(page, 900);
  await s.snap(page, "adm-detail");
  await page.context().close();
});

scene("library", ["lib-settings", "lib-catalogue", "lib-desk", "lib-loans", "lib-stock"], async (s) => {
  const page = await tabs(s, "/library", [
    ["lib.tab.settings", "lib-settings"],
    ["lib.tab.catalogue", "lib-catalogue"],
    ["lib.tab.desk", "lib-desk"],
    ["lib.tab.loans", "lib-loans"],
    ["lib.tab.stock", "lib-stock"],
  ]);
  await page.context().close();
});

scene("transport", ["trp-vehicles", "trp-routes", "trp-assign", "trp-roster"], async (s) => {
  const page = await tabs(s, "/transport", [
    ["trp.tab.vehicles", "trp-vehicles"],
    ["trp.tab.routes", "trp-routes"],
    ["trp.tab.assign", "trp-assign"],
    ["trp.tab.roster", "trp-roster"],
  ]);
  await page.context().close();
});

scene("hostel", ["hst-hostels", "hst-board", "hst-residents", "hst-register"], async (s) => {
  const page = await tabs(s, "/hostel", [
    ["hst.tab.hostels", "hst-hostels"],
    ["hst.tab.board", "hst-board"],
    ["hst.tab.residents", "hst-residents"],
    ["hst.tab.register", "hst-register"],
  ]);
  await page.context().close();
});

scene("inventory", ["inv-setup", "inv-items", "inv-movement", "inv-stock", "inv-assets"], async (s) => {
  const page = await s.desktop({ route: "/inventory" });
  const $ = by(s, page);
  await click($.tab("inv.tab.setup"));
  await settle(page, 800);
  await s.snap(page, "inv-setup");
  await click($.tab("inv.tab.items"));
  await settle(page, 800);
  await s.snap(page, "inv-items");
  await click(page.getByTestId("record-movement"));
  await s.snap(page, "inv-movement", { target: $.dialog() });
  await page.keyboard.press("Escape");
  for (const [tab, id] of [
    ["inv.tab.stock", "inv-stock"],
    ["inv.tab.assets", "inv-assets"],
  ]) {
    await click($.tab(tab));
    await settle(page, 900);
    await s.snap(page, id);
  }
  await page.context().close();
});

scene("coming soon", ["hr-soon"], async (s) => {
  const page = await s.desktop({ route: "/hr" });
  await s.snap(page, "hr-soon");
  await page.context().close();
});

scene("directory login", ["dir-card"], async (s) => {
  const page = await s.desktop({ route: "/settings" });
  const $ = by(s, page);
  await $.field("directory.enabled").check().catch(() => {});
  await s.snap(page, "dir-card", { target: $.card("directory.title"), maxHeight: 900 });
  await page.context().close();
});

scene("read-only api", ["api-card", "api-new"], async (s) => {
  const page = await s.desktop({ route: "/settings" });
  const $ = by(s, page);
  await s.snap(page, "api-card", { target: $.card("apikeys.title") });
  await click($.button("apikeys.add"));
  await s.snap(page, "api-new", { target: $.dialog() });
  await page.context().close();
});

// ---- Part 4: several PCs on the school network ---------------------------------------------

const FINGERPRINT = "3F:A2:9C:41:7B:D0:5E:88:12:C4:9A:6F:E1:03:B7:52:4D:9E:20:AF:68:C1:73:0B:9D:E4:51:26:8F:3A:C0:77";

scene("school server and clients", ["lan-server", "lan-network", "lan-client-find", "lan-client-confirm", "lan-client-card"], async (s) => {
  // The first start of the server PC.
  let page = await s.desktop({ mode: null, signedIn: false });
  let $ = by(s, page);
  await click(page.getByRole("button", { name: new RegExp(`^${s.L("mode.server.title")}`) }));
  await s.snap(page, "lan-server", { around: $.button("mode.server.start") });
  await page.context().close();
  // Settings → Network on the server.
  page = await s.desktop({
    route: "/settings",
    mode: { mode: "server", server_url: null, fingerprint: FINGERPRINT, listen_port: 8471 },
    network: { mode: "server", addresses: ["192.168.1.20"], port: 8471, fingerprint: FINGERPRINT, server_url: null },
  });
  $ = by(s, page);
  await s.snap(page, "lan-network", { target: $.card("network.title") });
  await page.context().close();
  // A teacher's PC: find the server, check the fingerprint, connect.
  page = await s.desktop({ mode: null, signedIn: false });
  $ = by(s, page);
  await click(page.getByRole("button", { name: new RegExp(`^${s.L("mode.client.title")}`) }));
  await settle(page, 900);
  await s.snap(page, "lan-client-find", { around: $.button("mode.client.search") });
  await click(page.getByText("DEMO Gulshan Model School").first());
  await settle(page, 700);
  await s.snap(page, "lan-client-confirm", { around: $.button("mode.client.connect") });
  await page.context().close();
  page = await s.desktop({
    route: "/settings",
    mode: { mode: "client", server_url: "https://192.168.1.20:8471", fingerprint: FINGERPRINT, listen_port: 8471 },
    network: { mode: "client", addresses: [], port: 0, fingerprint: FINGERPRINT, server_url: "https://192.168.1.20:8471" },
  });
  $ = by(s, page);
  await s.snap(page, "lan-client-card", { target: $.card("network.title") });
  await page.context().close();
});

// ---- Part 5: Madrasa Nexus -----------------------------------------------------------------

scene("hub first setup", ["hub-setup", "hub-login"], async (s) => {
  let page = await s.console({ hub: URLS.hubSetup, signedIn: false });
  const inputs = page.locator("main input");
  const values = [SETUP_TOKEN, "DEMO Head Office", "DEMO Administrator", "demo.admin", PASSWORD, PASSWORD];
  const n = await inputs.count();
  for (let i = 0; i < n && i < values.length; i++) {
    const type = await inputs.nth(i).getAttribute("type");
    if (type === "checkbox") continue;
    await inputs.nth(i).fill(values[i]);
  }
  await s.snap(page, "hub-setup", { around: page.locator("main form").first() });
  await page.context().close();
  page = await s.console({ signedIn: false });
  await page.locator('input[autocomplete="username"]').fill("demo.admin");
  await s.snap(page, "hub-login", { around: page.locator("main form").first() });
  await page.context().close();
});

scene("head office console", ["hub-schools", "hub-invite", "hub-joincode", "hub-school", "hub-reports", "hub-corrections", "hub-escrow", "hub-users", "hub-activity"], async (s) => {
  const page = await s.console({ route: "/schools" });
  const $ = by(s, page);
  await settle(page, 700);
  await s.snap(page, "hub-schools");
  await click($.button("hub.invite"));
  await page.getByLabel(s.L("hub.invite.school"), { exact: true }).fill("DEMO Gulshan Model School");
  await s.snap(page, "hub-invite", { target: $.dialog() });
  await click($.dialog().getByRole("button", { name: s.L("hub.invite.create"), exact: true }));
  await page.getByTestId("join-code").waitFor();
  s.joinCode = (await page.getByTestId("join-code").innerText()).trim();
  await s.snap(page, "hub-joincode", { target: $.dialog() });
  await page.keyboard.press("Escape");
  await settle(page, 400);
  await click(page.getByRole("button", { name: s.L("hub.schools.open"), exact: true }));
  await settle(page, 900);
  await s.snap(page, "hub-school");
  for (const [path, id] of [
    ["/reports", "hub-reports"],
    ["/corrections", "hub-corrections"],
    ["/escrow", "hub-escrow"],
    ["/users", "hub-users"],
    ["/activity", "hub-activity"],
  ]) {
    await page.evaluate((p) => {
      history.pushState({}, "", p);
      dispatchEvent(new PopStateEvent("popstate"));
    }, path);
    await settle(page, 900);
    await s.snap(page, id);
  }
  await page.context().close();
});

scene("a school joins the network", ["join-card", "join-done"], async (s) => {
  const page = await s.desktop({ route: "/settings" });
  const $ = by(s, page);
  const card = $.card("nexus.title");
  await card.getByLabel(s.L("nexus.hubUrl"), { exact: true }).fill(URLS.hub);
  await card.getByLabel(s.L("nexus.joinCode"), { exact: true }).fill(s.joinCode ?? "");
  await s.snap(page, "join-card", { target: card });
  await click(card.getByRole("button", { name: s.L("nexus.join"), exact: true }));
  await settle(page, 2500);
  await s.snap(page, "join-done", { target: $.card("nexus.title") });
  await page.context().close();
});

// ---- Part 6: the parent app ----------------------------------------------------------------

scene("parent app at the school", ["pa-access", "pa-give", "pa-slip", "pa-notices", "pa-notice-new", "pa-messages"], async (s) => {
  const page = await s.desktop({ route: "/parent-app" });
  const $ = by(s, page);
  await settle(page, 900);
  await click($.tab("pa.tab.access"));
  await settle(page, 700);
  await s.snap(page, "pa-access");
  await click(page.getByTestId("give-access"));
  const search = page.getByLabel(s.L("pa.access.search.label"), { exact: true });
  await search.fill("Khan");
  await settle(page, 1200);
  await s.snap(page, "pa-give", { target: $.dialog() });
  await click(page.locator('[data-testid^="choose-"]').first());
  await page.getByTestId("access-slip").getByTestId("access-code").waitFor({ timeout: 60000 });
  await settle(page, 500);
  await s.snap(page, "pa-slip", { target: $.dialog() });
  await page.keyboard.press("Escape");
  await click($.tab("pa.tab.notices"));
  await settle(page, 700);
  await s.snap(page, "pa-notices");
  await click(page.getByTestId("new-notice"));
  await s.snap(page, "pa-notice-new", { target: $.dialog() });
  await page.keyboard.press("Escape");
  await page.context().close();
  // Messages: the class teachers and the principal read them.
  const principal = await s.desktop({ user: await s.userWithRole("principal"), route: "/parent-app" });
  await click(by(s, principal).tab("pa.tab.messages"));
  await settle(principal, 900);
  await s.snap(principal, "pa-messages");
  await principal.context().close();
});

scene("parent app on the phone", ["parent-welcome", "parent-code", "parent-login", "parent-children", "parent-overview", "parent-attendance", "parent-results", "parent-fees", "parent-notices", "parent-messages"], async (s) => {
  let page = await s.parent({ signedIn: false, route: "/welcome" });
  await page.evaluate(() => localStorage.removeItem("madrasa.parent.hub"));
  await page.goto(`${URLS.parent}/#/welcome`);
  await settle(page, 600);
  await s.snap(page, "parent-welcome");
  await page.goto(`${URLS.parent}/#/code`);
  await settle(page, 600);
  await page.getByTestId("hub-address").fill(URLS.hub).catch(() => {});
  await page.getByTestId("access-code").fill("7PR7-6Z93-BC1D").catch(() => {});
  await s.snap(page, "parent-code");
  await page.context().close();
  page = await s.parent({ signedIn: false, route: "/login" });
  await page.getByTestId("phone").fill("+923001234567");
  await s.snap(page, "parent-login");
  await page.context().close();
  page = await s.parent();
  await settle(page, 1000);
  await s.snap(page, "parent-children");
  const child = await page.locator('a[href^="#/child/"]').first().getAttribute("href");
  for (const [tab, id] of [
    ["", "parent-overview"],
    ["/attendance", "parent-attendance"],
    ["/results", "parent-results"],
    ["/fees", "parent-fees"],
    ["/notices", "parent-notices"],
    ["/messages", "parent-messages"],
  ]) {
    await page.goto(`${URLS.parent}/${child}${tab}`);
    await settle(page, 1000);
    await s.snap(page, id);
  }
  await page.context().close();
});

// ---- last: the end of the year (changes the demo school's year) ----------------------------

scene("year-end rollover", ["rollover-open", "rollover-exceptions", "rollover-done"], async (s) => {
  // A tall window: the dialog lists every section and ends with its button.
  const page = await s.desktop({ route: "/settings", viewport: { width: 1280, height: 1900 } });
  const $ = by(s, page);
  await click($.button("rollover.button"));
  await settle(page, 1200);
  await s.snap(page, "rollover-open", { target: $.dialog(), maxHeight: 720 });
  const retain = page.getByLabel(s.L("rollover.retain"), { exact: true });
  await retain.scrollIntoViewIfNeeded();
  await retain.fill("DEMO01-2015-00001");
  await s.snap(page, "rollover-exceptions", { target: retain.locator("xpath=ancestor::div[1]"), margin: 16 });
  await $.button("rollover.confirm").evaluate((b) => b.click());
  await settle(page, 6000);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.goto(`${URLS.desktop}/settings`);
  await settle(page, 900);
  await s.snap(page, "rollover-done", { target: $.card("academics.years") });
  await page.context().close();
});
