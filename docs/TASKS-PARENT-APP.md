# Madrasa parent app — alpha for Android

Goal: an Android app for parents, in English and Urdu, in a new `apps/parent` (Tauri 2
mobile, the desktop app's design system). A parent signs in, picks a child, and sees
attendance, results and report cards, fee status and vouchers, school notices, and a
one-to-one conversation with the class teacher that the principal can read. The app talks
to the Madrasa Nexus hub; the hub gets the endpoints it needs, with permissions so that a
parent only ever sees their own children. CI builds the APK and releases carry it.

The design is in `docs/adr/0006-parent-app.md`. Read it first.

**Lite and Standard keep working unchanged.** A school that has not joined a hub gets no
new screens, no new behaviour and no network calls; every existing test keeps passing.
The parent features appear in a school only after it joins a hub.

Not in this alpha: iOS, push notifications, payments, PDF downloads in the app,
attachments, messages to anyone but the class teacher, SMS or email, self-registration.

## How to work

- **Start every stage from the latest `main`**, use the **next free migration number after
  pulling `main`**, and **rebase on `main` before opening the PR**.
- One stage at a time, in order. One branch (`dev/parent-N-name`) and one pull request per
  stage. When CI is green, merge it into `main` and continue with the next stage.
- Follow every rule in CLAUDE.md. Demo data only. No secret, token, password, access code
  or signing key is committed or logged.
- Permissions are enforced in the backend. Every parent endpoint is tested for the three
  ways of reaching someone else's child: another family in the same school, another
  school, and a link the school has ended.
- Money is `NUMERIC`/`rust_decimal` and travels as text; never floats.
- Every string goes through the translation layer (English and Urdu); layouts work right
  to left.
- Tick `[x]` only when built and tested. Keep docs/PROGRESS.md updated after each stage;
  log blockers in docs/BLOCKERS.md and move on rather than guess.

## Stage 0 — Design

- [x] `docs/adr/0006-parent-app.md`: the app talks only to the hub; parent accounts at the
      hub, opened by the school with one-time access codes; double scoping to the parent's
      own children; approved results only; notices and messages as school records that
      sync; messages add-only and not instant; signing
- [x] This task list

## Stage 1 — School records: notices and messages

- [ ] School migration: `notices` (title, body, audience: whole school or chosen classes,
      published and expiry dates), `message_threads` (one per student and guardian),
      `messages` (who wrote it, text, when); ids, `school_id`, sync columns, soft delete
- [ ] Sync policy for the three tables: notices by field; threads by field (each side
      writes only its own "read up to" mark); messages add-only like receipts. Unit and
      database tests: a parent's message written at the hub and a teacher's written at the
      school both survive a sync, in order
- [ ] Feature `parent_app` (all editions; active only while the school is joined to a hub)
      and permissions: `parents.manage` (access codes), `notices.manage`, `messages.reply`
      (class teachers, for their own sections), `messages.view_all` (principal); granted to
      the built-in roles by the migration; permissions document
- [ ] School API: notices (list, create, edit, publish, withdraw); messages (threads a
      teacher may see, read a thread, reply, mark read); the principal reads all and cannot
      reply; a teacher cannot open a thread of another teacher's section. All refused with
      a plain message while the school is not joined. Audit log entries
- [ ] Tests: permissions for each role, the class-teacher rule, the not-joined refusal,
      Lite and Standard unchanged when not joined

## Stage 2 — Hub: parent accounts and access codes

- [ ] Hub migration: `hub_parents` (phone, password hash, lockout), `hub_parent_links`
      (account, hub school, guardian id), `hub_parent_access_codes` (hash, expiry, used),
      `hub_parent_sessions`
- [ ] For the school (sync token): issue an access code for a guardian, list which
      guardians have access, end a link. School API and its permission `parents.manage`
- [ ] For the parent: activate with a code (new account, or add to an existing one, or
      set a new password), sign in, sign out, who am I. Lockout after repeated failures.
      Parent sessions open nothing in the console and nothing in the school sync API
- [ ] Links end when the school ends them, when the guardian record is removed, and when
      the school leaves or is removed from the hub; purging a school deletes them
- [ ] Tests: a code works once and expires; wrong code and wrong password lock; two
      schools on one account; a console session or sync token is refused on parent routes
      and the reverse; hub audit entries without secrets

## Stage 3 — Hub: what a parent can see

- [ ] Move the report card calculation from the school API into `madrasa_db` (one piece of
      code for the school and the hub); the school's report card tests keep passing
- [ ] `/parent/children` and, for one child: attendance by month with a summary; results
      (approved marks sheets only) and the report card; fees (vouchers with lines, paid,
      due, receipts); notices for the child's class; the conversation with the class
      teacher (read, send, mark read)
- [ ] Every response says when the school last synced
- [ ] Cross-origin access for the app (bearer tokens only, no cookies)
- [ ] Tests: each figure is compared with the school's own database; unapproved marks are
      never shown; the three isolation cases on every endpoint; a parent's message reaches
      the school at its next sync and the teacher's answer comes back

## Stage 4 — School desktop: access codes, notices, messages

- [ ] A "Parent app" area in the desktop app, shown only when the school is joined to a
      hub: give a guardian access (the code is shown once, with a printable slip), see who
      has access, end access
- [ ] Notices: write, publish for the whole school or chosen classes, withdraw
- [ ] Messages: a class teacher sees and answers the conversations of their sections;
      the principal sees all, read-only; unread counts; sending triggers a sync
- [ ] English and Urdu; screen tests

## Stage 5 — The parent app

- [ ] `apps/parent`: Vite, React, TypeScript, the desktop app's components, tokens and
      translation layer; Tauri 2 mobile shell (`src-tauri`) in the workspace
- [ ] Screens: hub address and access code (first time), sign in, children, and for a
      child: overview, attendance, results and report card, fees and vouchers, notices,
      messages; language switch; sign out; "last updated" from the school's last sync;
      clear states for no connection and for a school that has stopped syncing
- [ ] English and Urdu, right to left; phone-sized layout; keyboard and screen-reader labels
- [ ] Screen tests; tried in a phone-sized browser against a local hub with a demo school

## Stage 6 — Android builds

- [ ] CI builds the APK on every pull request and keeps it as an artifact
- [ ] The Release workflow builds the APK and attaches it to the release
- [ ] Signing with the repository's keystore secrets when present, otherwise a throwaway
      key for that run, stated in the log; `docs/RELEASING.md` says how to create the
      keystore and add the secrets
- [ ] Honest note of what could not be checked from the build environment (running the
      APK on a phone)

## Stage 7 — Close-out

- [ ] Demo: the demo school and the hub's demo network have notices, a conversation and
      a guardian ready to be given access
- [ ] TESTING.md section for the parent app (install the APK, get a code, each screen,
      messaging both ways, a second child, Urdu)
- [ ] Spec sections (tables, endpoints, permissions); `docs/permissions.md`
- [ ] docs/PROGRESS.md: summary for the owner, what to test by hand, decisions left

## Done when

A school joined to a hub can give a parent an access code; the parent installs the APK
from the release, signs in, picks a child and sees that child's attendance, approved
results and report card, vouchers and notices, and exchanges messages with the class
teacher, which the principal can read; nothing of any other family or school can be
reached; and every existing Lite, Standard and Nexus test still passes.
