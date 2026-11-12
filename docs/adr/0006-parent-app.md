# ADR 0006: Parent app — accounts at the hub, data from the hub's copy, messages by sync

- Status: accepted
- Date: 2026-11-12

## Context

Parents should see their children's attendance, results and report cards, fees and
vouchers, read school notices and exchange messages with the class teacher, from an
Android phone, in English and Urdu.

A school's records live on the school's own computer, usually behind a home-grade internet
connection and switched off at night. A phone cannot reach it. The only always-on place
that has the records is the Madrasa Nexus hub (ADR 0005), which holds a synced copy of each
joined school in a schema of its own.

## Decision

### 1. The app talks only to the hub

The parent app is a client of the hub (`/api/nexus/v1/parent/...`). A school offers the app
to its parents by joining a hub; a school that has not joined is unchanged and has no
parent features. What a parent sees is the hub's copy **as of the school's last sync**
(every five minutes while the school's computer is on), and the app says so.

### 2. Parent accounts live at the hub, opened by the school

- A parent account (`hub_parents`) is identified by a phone number and a password
  (Argon2id, the same lockout rules as other accounts). It belongs to the hub, not to a
  school: password hashes never enter a school's schema and are never synced.
- An account is linked to one or more **guardian records** (`hub_parent_links`: hub school
  + the school's guardian id). A parent with children in two schools of the network links
  both and sees all the children in one place.
- Only a school can open access. In the desktop app, someone with the new permission
  `parents.manage` asks the hub (with the school's sync token) for an **access code** for
  one guardian. The code is shown once, printed on a slip with the hub's address, works
  once and expires after 14 days; the hub keeps its hash only. The parent enters it in the
  app, chooses a phone number and password (or signs in to an existing account), and the
  link is made. The school can see which guardians have access and can end a link.
- There is no self-registration and no password recovery by SMS or email in this alpha
  (the hub sends no messages). A parent who forgets the password gets a new access code
  from the school, which lets them set a new password.

### 3. A parent sees only their own children

Every parent endpoint is scoped twice, in the backend:

1. to the links of the signed-in account (hub school + guardian id), and
2. inside that school's schema, to students joined to that guardian by
   `student_guardians`, checked on every request.

A student id from another family, another school's id, or a guardian link the school has
ended gives `404`. Tests prove each case. The school's schema is selected the same way as
for sync (ADR 0005 §2), so a query cannot reach another school's tables.

### 4. What is shown

- **Attendance**: the child's marks by month and a summary.
- **Results and report cards**: only marks from **approved** marks sheets. The report card
  is the same calculation the school's own report card uses, moved from the school API into
  `madrasa_db` so both sides run one piece of code. It is shown on screen; PDF download is
  not in this alpha (the PDF typesetter stays in the school app).
- **Fees**: the child's vouchers with their lines, what is paid, what is due, receipts. No
  payment is taken in the app.
- **Notices**: a new `notices` table in the school's records, written in the desktop app
  for the whole school or for chosen classes, synced up, read in the app.
- **Messages**: one conversation per child and guardian with **the class teacher of the
  child's current section**. New tables `message_threads` and `messages` in the school's
  records. A parent's message is written into the school's copy at the hub and reaches the
  school at its next sync; the teacher answers in the desktop app and the answer goes up
  the same way. Messages are only ever added (the sync policy treats them like receipts:
  both sides' rows are kept), so nothing a parent or teacher wrote can be overwritten. The
  principal (permission `messages.view_all`) can read every conversation and does not
  reply. Messaging is therefore **not instant**: a message takes up to one sync interval
  each way, and the app says "sent, the teacher will see it when the school next syncs".

### 5. The app

`apps/parent`: Tauri 2 mobile for Android, React and TypeScript, using the desktop app's
components, design tokens and translation layer (English and Urdu, right to left) by
importing them from `apps/desktop/src`; its strings are `parent.*` keys in the same
dictionaries. It keeps the session token in the app's private web storage. It refuses a
hub address that is not `https://` (Android itself blocks plain HTTP in release builds).

### 6. Builds

CI builds the APK on every pull request. The Release workflow attaches it to the release.
An APK must be signed to install: the workflows sign with the keystore held in repository
secrets when they exist, and otherwise with a throwaway key made for that run, and say
which in the build log and the release notes. A throwaway key means each build must be
installed fresh (Android refuses an update signed by another key). Creating the real
keystore and adding the secrets is a step for the owner.

## Consequences

- No parent feature works for a school that is not joined to a hub, by design.
- Freshness depends on the school's computer being on and syncing; the app shows the last
  sync time.
- The hub now holds parent credentials and serves families directly from the internet, so
  it must run over HTTPS and be kept patched. Parent sessions are separate from head
  office sessions and can open nothing in the console.
- The school gains two small screens (notices, messages) and one action (access codes),
  visible only once it has joined a hub.
- Not in this alpha: iOS, push notifications, payments, PDF downloads, attachments,
  messages to anyone but the class teacher, SMS or email.
