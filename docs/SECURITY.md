# Security: threat model, review and remaining risks

Reviewed on 25 Nov 2026 against the code that becomes the first pilot build (version 0.5.1
plus this review). To report a vulnerability, see `SECURITY.md` at the top of the repository.

This document says what Madrasa protects, from whom, what the review found, what was fixed,
and what is still a risk. It does not claim more than was checked: each section says how it
was checked (a test that runs in CI, a reading of the code, or a look at a real install).

## 1. What is protected

| Asset | Where it lives |
| --- | --- |
| Student, guardian and staff records (names, addresses, phone numbers, ID numbers, photos, marks, attendance) | The school's PostgreSQL database on the school's PC; a copy at the hub when the school has joined a network |
| Money records (vouchers, receipts, concessions) and the audit log | Same |
| Logins: password hashes, session tokens | Database (hashes only); the signed-in token in the app's WebView storage |
| Backups | Encrypted `.mbk` files in the backup folder (USB drive, network folder) |
| Keys: backup key set, hub link token, TLS key of the school server, the offline queue's key, the bundled database's password | Database settings (wrapped), and files in Madrasa's data folder |
| Parent accounts and what a parent may see | The hub |

## 2. Who might attack, and what is out of scope

| Who | What they can do | In scope? |
| --- | --- | --- |
| A staff member with a login | Call the API with their own token, including routes their screens do not show | Yes |
| Someone on the school's network without a login (a pupil's phone on the Wi-Fi) | Reach the school server's port, try logins, watch traffic | Yes |
| Another account on the same PC | Read files the first account left readable | Yes |
| Someone who gets a backup file, or an exported spreadsheet | Try to open it; plant content that harms whoever opens it | Yes |
| Someone who hands the school a file to import | A hostile spreadsheet | Yes |
| Anyone on the internet | Reach a hub: try console logins, parent logins, join and access codes | Yes |
| A parent with an account | Call the hub's API with their token for children that are not theirs | Yes |
| A school joined to a hub | Send crafted sync data to reach another school's copy | Yes |
| The administrator of the school, or of the PC | Everything: they hold the data. The audit log records what is done through Madrasa, but someone with the database password can change the database directly | **No** |
| Someone with the PC in their hands, unlocked, or its disk | Read the data folder. Use full-disk encryption (BitLocker, LUKS) | **No** |
| Malware running as the same user | Read everything that user can | **No** |

## 3. Findings and fixes

Severity is for a school in the pilot: **high** (someone gets data or access they must not
have), **medium** (needs another weakness or an insider), **low** (hardening).

| # | Finding | Severity | Status |
| --- | --- | --- | --- |
| F1 | The bundled database's password was also written to `pg/.pgpass`, readable by every account on the PC (found testing v0.5.0) | High | Fixed in the 0.5.1 fixes: not written, old file removed, whole data folder owner-only, checked at every start |
| F2 | The WebView's own folder (`~/.local/share/org.quietbyte.madrasa`: the signed-in session token, the drafts and their key) was `drwxr-xr-x` on Linux | High | Fixed: created closed and closed again at every start |
| F3 | A password reset did not end the user's sessions: whoever was signed in with the old password stayed in | High | Fixed: a reset ends them all |
| F4 | Only an administrator could set a password, so the administrator knew every user's | Medium | Fixed: every user can change their own (user menu → Change password); the others' sessions end |
| F5 | The login told real usernames from made-up ones: only real accounts locked on the fifth wrong password, and a disabled account said "disabled" without a password (school API, hub console, parent login) | Medium | Fixed: unknown names are counted and locked the same way; "disabled" only after the right password |
| F6 | A session used at least every twelve hours never ended | Medium | Fixed: seven days at most, then sign in again |
| F7 | Vulnerable dependencies: `calamine` 0.29, `zip` 2.6/4.2, `quick-xml` 0.37 (crashes and memory exhaustion on hostile spreadsheets), `url`, `cookie`; `tinypool` (critical, test tooling only) | High for the spreadsheet readers | Fixed by upgrades; `cargo audit`, `cargo deny` and `pnpm audit` now run in CI |
| F8 | A tiny `.xlsx` with one cell in the far corner of a sheet, or a workbook that unpacks to gigabytes, would make the server run out of memory and stop | Medium (needs a login that may import) | Fixed: workbooks are measured before they are opened |
| F9 | CSV exports wrote text starting with `=`, `+`, `-` or `@` as it was; a spreadsheet program runs such a cell as a formula on the PC of whoever opens the file | Medium | Fixed: such text is written as text. Excel workbooks were never affected |
| F10 | Who may call a route was decided inside each handler, after the request body was read; nothing checked the 386 routes as a whole against the permission document | Medium | Fixed: one table, enforced before any handler; a test calls every route as every role |
| F11 | The report list refused store keepers and transport and hostel readers although they may run their module's reports; `GET /users` asked for `users.manage` where the document says `users.read`; editing the school profile asked for `settings.manage` where the document says `school.manage` | Low (too strict, never too loose) | Fixed |
| F12 | The school API answered any web origin (`Access-Control-Allow-Origin: *`) although a school server listens on the school network | Low (logins are bearer tokens, not cookies) | Fixed: only the app's own origins |
| F13 | The desktop app's WebView had no content security policy | Low (no place was found where data becomes markup) | Fixed: scripts from the app only, connections to this PC only |
| F14 | Hub: nothing limited how often one address could try a login, a join code or an access code beyond the per-account lock | Low (codes are 60 and 160 random bits) | Fixed: 60 attempts per address and door per five minutes, then `429` |
| F15 | The desktop app's error screen said "see the log file", and there was none | Low | Fixed: `logs/madrasa.log` in the data folder, owner-only |
| F16 | When the server ended a session (password changed, session ran out) an open app stayed on its screen showing errors instead of asking to sign in | Low | Fixed: it returns to the login |

Nothing was found, by the tests below, that let one role read or change what the permission
document does not give it, one school reach another's copy at the hub, or a parent reach a
child that is not theirs.

## 4. The review, area by area

### 4.1 Dependencies

- `cargo audit` (RustSec advisories against `Cargo.lock`), `cargo deny check` (advisories on
  the real dependency graph for Linux, Windows and Android, licences, sources) and
  `pnpm audit --audit-level high` run in the CI job **Dependency audit**. A new advisory
  fails the build until it is fixed or explained.
- Three advisories are set aside, with reasons, in `.cargo/audit.toml` and `deny.toml`:
  `quick-xml` 0.38 (only inside Typst's citation-style parser, which Madrasa never feeds),
  and `libsqlite3-sys` (named in the lock file by an sqlx driver Madrasa does not build).
- "Unmaintained" notices (the GTK3 bindings Tauri 2 needs on Linux, `yaml-rust`, `paste`,
  `bincode` and others, all indirect) do not fail the build. Review them at each release.
- Licences: everything shipped is under a licence compatible with Apache-2.0 (the list is
  in `deny.toml`); crates come from crates.io only.

### 4.2 Authentication

- **Passwords**: Argon2id, 19 MiB, 2 passes, a random salt per password (the OWASP minimum
  setting), at least 10 characters. No other rule and no second factor (see §5).
- **Lockout**: five wrong passwords lock an account for fifteen minutes; names that are not
  accounts behave the same (F5). Changing one's own password counts wrong "current
  password" attempts the same way.
- **Sessions**: a random 256-bit token; the server keeps only its SHA-256. Ends on logout,
  after twelve hours unused, seven days after sign-in (F6), when the password changes (F3,
  F4), and at once when the account is disabled.
- **Messages**: a wrong username and a wrong password give the same answer, at the same
  cost (a real Argon2 check against a dummy hash).
- Tests: `crates/api/tests/security.rs`, `crates/nexus/tests/security.rs`,
  `crates/nexus/tests/hub.rs`, `crates/nexus/tests/parents.rs`.

### 4.3 Authorisation

- `crates/api/src/authz.rs` lists every route with the permission it needs. A middleware
  refuses a request there, before its body is read; a route missing from the list is refused
  outright. Handlers keep their own checks and the finer rules (a teacher's own sections).
- `docs/permissions.md` carries the same list. `crates/api/tests/authz_matrix.rs` reads the
  routes from the source, the list from the code and the list from the document, fails if
  they differ, then calls each of the 387 routes without a login and as each of the six
  built-in roles and checks every answer.
- Not covered by that test: scoping inside a permission (teachers and their sections, the
  owner of a saved report). The module tests cover those; they were not re-audited here.
- The read-only API (`/api/ro/v1`) is separate: API keys with scopes, an address allow-list
  and a rate limit (`crates/api/tests/readonly_api.rs`).

### 4.4 The hub

- **Tenant isolation**: each school's copy is its own PostgreSQL schema, named from an id the
  hub makes up; a school's token opens only its own (`schools.rs`:
  `two_schools_with_identical_data_never_see_each_other`; `hub.rs`:
  `hub_tables_live_in_their_own_schema`).
- **Parent isolation**: every parent route resolves the child through the parent's own
  links (`parent_data.rs`: `a_parent_reaches_their_own_children_and_nobody_elses`).
- **Attempts**: per-account locks, and per-address limits at the six doors that take a
  secret (F14). Behind a reverse proxy set `NEXUS_TRUSTED_PROXIES` (the cloud templates and
  the Compose file do).
- **Browser side**: a strict content security policy, `nosniff`, no framing, no referrer,
  `Permissions-Policy`, `Cross-Origin-Opener-Policy`, and HSTS when the public address is
  `https://`. The console's API allows no other origin; the parent app's routes allow any
  origin because they trust only the token in the `Authorization` header and use no cookies.
  The health check (`/health`: is the hub there, which version) also answers any origin: the
  parent app asks it from its own pages, and it holds nothing private.
- **Codes**: join codes are 160 random bits, used once; parent access codes are 60 random
  bits, used once, valid fourteen days.

### 4.5 Files

- Everything Madrasa writes under its data folder is for the owning user only: `0600` and
  `0700` on Linux; on Windows an access list of the user, SYSTEM and Administrators with
  inheritance off. Checked and put right at every start (`crates/api/src/private.rs`,
  `crates/api/tests/embedded.rs` against the real bundled PostgreSQL).
- The WebView's folders are closed the same way on Linux (F2). On Windows they are under
  the user's `AppData`, which other accounts cannot open; nothing is changed there.
- Backups are encrypted wherever they are written. Inside the data folder they are private
  too; in a folder the school chose (a USB drive) they keep that folder's permissions.
- **Logs** (`logs/madrasa.log`, owner-only, restarted at 5 MB): start-up, addresses the
  app listens on, slow SQL statements (the statement text, never the values), refused
  permissions (username and route), sync problems. Not logged: passwords, tokens, the
  database address, request bodies, records. With `RUST_LOG=debug` request paths are
  logged, and a path can contain what was typed in a search box; do not run a school that
  way.
- The hub prints its one-time setup token once in its own log when none was configured;
  the token stops working when setup is done.

### 4.6 Cryptography

- **Backups**: a random 256-bit data key, wrapped by a key derived with Argon2id (64 MiB,
  3 passes) from the backup password and again from a random 160-bit recovery key; each
  backup file has its own random key, sealed with X25519 so unattended backups need no
  password; contents under AES-256-GCM. A changed file fails authentication and is refused.
- **Offline queue and cache** (LAN clients): AES-256-GCM under a random key file beside
  them. This protects copies of the files, not against the same OS user (§5).
- **Drafts**: AES-GCM through the WebView's Web Crypto, under a key that cannot be exported.
- **School server on the network**: TLS with a certificate made on the server PC; clients
  pin its fingerprint at first connection, which a person compares by eye.
- No key is written in the code. The demo school and the demo network create their own
  random keys; their only fixed secrets are the passwords the tester types.
- Only audited crates do the work (`argon2`, `aes-gcm`, `x25519-dalek`, `sha2`, `rustls`).

### 4.7 Input handling

- **SQL**: every statement goes through sqlx with bound values. sqlx refuses a statement
  built at run time unless it is marked; all 177 marked statements were read: what is put
  into the text is a constant column list, a table name from a fixed list, or a name checked
  against the database's own catalogue (backup restore, sync). No user value reaches SQL
  text. Restore now quotes table names properly as well.
- **Import files**: at most 20 MB; zip-based workbooks are unpacked into nothing first and
  refused over 300 MB or 2,000 parts; an `.xlsx` sheet is read as a stream and refused if it
  spans more than a real list (F8). Cells are read as values; formulas are never evaluated.
- **Exports**: CSV cells that a spreadsheet would run are written as text (F9).
- **PDFs**: every value is written into the Typst source as an escaped string literal;
  tests typeset reports, vouchers and report cards whose names try to break out.
- **The app's screens**: React escapes everything; no place sets HTML from data.

## 5. Remaining risks

Ordered by how much they matter for a pilot.

1. **The tag `v0.5.1-alpha` does not contain F1 or any other 0.5.1 fix.** It was made on a
   commit that only changed the version number; the fixes were still on their branch. No
   build from that tag should go to a school. See docs/PROGRESS.md for the decision.
2. **First-run setup is open to whoever reaches the server first.** Until a school has been
   set up, anyone on the network who reaches a PC running as *school server* can create the
   school and its administrator. Set the school up before connecting the PC to the school's
   network, or on a network you trust.
3. **Keys are beside the data.** The database password, the TLS key and the offline queue's
   key are files in the data folder, owner-only. Another program running as the same user,
   or anyone with the disk, can read them. Use full-disk encryption on school PCs.
4. **The session token is in the WebView's storage**, not in the system keychain. It is
   now owner-only (F2) and ends after seven days at most.
5. **No second factor, no check against known-leaked passwords.** A ten-character minimum
   and the lock are all that stands between a guessed password and an account.
6. **Anyone on the school network can lock an account** by sending five wrong passwords
   for a known username (for fifteen minutes). The school API has no per-address limit; the
   hub has.
7. **`.xls` and `.ods` imports are measured less strictly than `.xlsx`**: the size limit
   and, for `.ods`, the unpacking limit apply, but not the sheet-span check. A hostile file
   of those kinds, from a login that may import, could still use a great deal of memory.
8. **The far-corner workbook (F8) was established by reading the spreadsheet library's
   code** (it allocates one slot per cell of the sheet's bounding rectangle), not by letting
   it take a server down. The guard is tested; the crash it prevents was not reproduced.
9. **Windows was not checked by hand.** The access-list code runs in CI's Windows tests; no
   Windows PC with a second account was used to try to read the data folder.
10. **The content security policy of the desktop app (F13) is checked only on Linux**, in
    the lab, not in WebView2 on Windows.
11. **Hub attempt limits are counted in memory per hub process.** Two hub instances behind
    a balancer each allow the full number. The hub's console sessions have no seven-day
    cap, and hub users cannot change their own password (an administrator resets it).
12. **A backup password in the wrong hands** opens every backup, and a crafted backup
    restored by someone who knows it can put any rows into the database (restore is meant
    to replace everything). Only administrators can restore.
13. **Installers are not signed**, so nothing proves a download is ours (Windows also warns).
14. **Unmaintained indirect dependencies** (§4.1) and Typst's citation parser stay until
    their upstreams move.
15. **A parent's phone number that already has an account** is confirmed as such to someone
    holding a valid, unused access code for another child. The code is the gate.
