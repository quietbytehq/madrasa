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

scene("the screen at a glance", ["overview-dashboard", "overview-search", "overview-usermenu", "overview-help"], async (s) => {
  const page = await s.desktop();
  const $ = by(s, page);
  await s.snap(page, "overview-dashboard");
  await page.keyboard.press("Control+k");
  await page.waitForTimeout(300);
  await page.keyboard.type("Ali");
  await page.waitForTimeout(900);
  await s.snap(page, "overview-search");
  await page.keyboard.press("Escape");
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
