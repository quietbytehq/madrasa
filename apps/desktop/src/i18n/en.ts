/**
 * English strings. This file defines the set of translation keys; every other language
 * must provide the same keys (enforced by the Translation type and a test).
 */
export const en = {
  // App
  "app.name": "Madrasa",
  "app.tagline": "School information system",
  "app.comingSoon": "Coming soon",
  "app.comingSoonBody": "This module is not available yet. It will arrive in a later release.",
  "app.loading": "Loading…",
  "app.error.generic": "Something went wrong. Please try again.",
  "app.error.network": "Cannot reach the Madrasa service. Is the application still starting?",

  // Common actions
  "action.save": "Save",
  "action.cancel": "Cancel",
  "action.close": "Close",
  "action.continue": "Continue",
  "action.back": "Back",
  "action.finish": "Finish",
  "action.retry": "Retry",
  "action.print": "Print",
  "action.search": "Search",
  "action.clear": "Clear",
  "action.logout": "Log out",
  "action.login": "Log in",
  "action.dismiss": "Dismiss",

  // Table
  "table.empty": "Nothing to show",
  "table.filter": "Filter rows",
  "table.rowsPerPage": "Rows per page",
  "table.page": "Page {page} of {pages}",
  "table.showing": "Showing {from}–{to} of {total}",
  "table.previous": "Previous page",
  "table.next": "Next page",
  "table.sortAsc": "Sorted ascending",
  "table.sortDesc": "Sorted descending",

  // Dialog / toast
  "dialog.close": "Close dialog",
  "toast.close": "Dismiss notification",

  // Navigation
  "nav.dashboard": "Dashboard",
  "nav.students": "Students",
  "nav.attendance": "Attendance",
  "nav.examinations": "Examinations",
  "nav.fees": "Fees & Finance",
  "nav.teachers": "Teachers",
  "nav.reports": "Reports",
  "nav.backup": "Backup",
  "nav.settings": "Settings",
  "nav.campuses": "Campuses",
  "nav.parents": "Parents",
  "nav.network": "Network",
  "nav.modules": "Modules",
  "nav.collapse": "Collapse sidebar",
  "nav.expand": "Expand sidebar",
  "nav.skipToContent": "Skip to content",

  // Top bar
  "topbar.search": "Search… (Ctrl+K)",
  "topbar.searchTitle": "Search",
  "topbar.searchHint": "Search across students, staff and fees will arrive with those modules.",
  "topbar.campus": "Campus",
  "topbar.notifications": "Notifications",
  "topbar.notificationsEmpty": "No notifications yet.",
  "topbar.help": "Help",
  "topbar.helpBody":
    "Madrasa is in pre-alpha. Documentation and in-app help will be added as modules are completed.",
  "topbar.userMenu": "User menu",
  "topbar.language": "Language",
  "topbar.theme": "Theme",
  "topbar.theme.system": "Follow system",
  "topbar.theme.light": "Light",
  "topbar.theme.dark": "Dark",
  "lang.en": "English",
  "lang.ur": "اردو",

  // Dashboard
  "dashboard.title": "Dashboard",
  "dashboard.welcome": "Welcome, {name}",
  "dashboard.card.students": "Students",
  "dashboard.card.attendanceToday": "Attendance today",
  "dashboard.card.feesCollected": "Fees collected this month",
  "dashboard.card.staff": "Staff",
  "dashboard.card.placeholder": "Data will appear once the module is available.",
  "dashboard.recentActivity": "Recent activity",
  "dashboard.recentActivityEmpty": "Activity from your school will be listed here.",
  "dashboard.edition": "Edition",
  "edition.lite": "Lite",
  "edition.standard": "Standard",
  "edition.nexus": "Nexus",

  // Auth
  "login.title": "Log in to Madrasa",
  "login.username": "Username",
  "login.password": "Password",
  "login.submit": "Log in",
  "login.failed": "The username or password is incorrect.",
  "login.locked":
    "This account is temporarily locked after too many failed attempts. Try again later.",
  "login.inactive": "This account is disabled. Contact your administrator.",

  // Setup wizard
  "setup.title": "Set up Madrasa",
  "setup.intro":
    "Welcome. A few details are needed before the first login. Everything can be changed later in Settings.",
  "setup.step.school": "School",
  "setup.step.year": "Academic year",
  "setup.step.admin": "Administrator",
  "setup.step.backup": "Backup security",
  "setup.step.done": "Recovery key",
  "setup.school.name": "School name",
  "setup.school.code": "School code",
  "setup.school.codeHint": "Short unique code, letters, digits and hyphens, for example KHI-MC. It also prefixes student numbers.",
  "setup.school.city": "City",
  "setup.year.startMonth": "Academic year starts in",
  "setup.year.april": "April",
  "setup.year.august": "August",
  "setup.year.currency": "Currency",
  "setup.admin.fullName": "Full name",
  "setup.admin.username": "Username",
  "setup.admin.password": "Password",
  "setup.admin.passwordConfirm": "Confirm password",
  "setup.admin.passwordHint": "At least 10 characters.",
  "setup.backup.password": "Backup password",
  "setup.backup.passwordConfirm": "Confirm backup password",
  "setup.backup.hint":
    "Backups are encrypted with this password. A recovery key will be generated in case it is forgotten.",
  "setup.done.title": "Save your recovery key",
  "setup.done.body":
    "This key can unlock your backups if the backup password is lost. It is shown only once. Print it or write it down and keep it somewhere safe.",
  "setup.done.ack": "I have saved the recovery key in a safe place.",
  "setup.done.go": "Go to login",
  "setup.error.passwordMismatch": "The passwords do not match.",
  "setup.error.required": "This field is required.",
  "setup.error.passwordShort": "Use at least 10 characters.",
  "setup.error.codeFormat": "Use 2–12 letters, digits or hyphens.",
  "setup.summary": "Summary",

  // Validation / misc
  "field.optional": "optional",
  "misc.demoBanner": "DEMO data — this school is not real.",
} as const;

export type TranslationKey = keyof typeof en;
export type Translation = Record<TranslationKey, string>;
