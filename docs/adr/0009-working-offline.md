# ADR 0009: Working offline

- Status: accepted
- Date: 2026-11-13

## Context

Schools run Madrasa on a school server with other PCs connected over the school network
(client mode), or on a single PC. Power cuts, switched-off routers and a server PC that
restarts are normal. Today a client PC that loses the server shows errors and loses whatever
was being entered; an answer lost half way through leaves the user unsure whether a payment
was recorded, so it may be entered twice.

The owner's goal: no school ever loses work because the network or the server dropped.

## Decision

1. **The outbox lives in the client's loopback proxy (Rust), not in the web UI.** Every
   request of a client PC already passes through it; it survives UI crashes and restarts, it
   can be tested end to end against the real server, and it works the same for every screen
   without changing each one. It serves the last copy of what screens read, queues changes
   in order, and sends them when the server answers.
2. **Exactly once by `Idempotency-Key`, on the server.** The proxy puts a key on every change
   from the first attempt; the server keeps the first answer for 30 days and returns it for
   repeats. A change whose answer was lost is queued under the same key. Queued changes are
   also de-duplicated in the outbox (Save pressed twice).
3. **Conflicts follow the sync rules (ADR 0005) with the school server in head office's
   place.** Field-level edits: the server's value wins a clash and the lost value is shown.
   Attendance: the later mark stands, decided on the server from each entry's time; a
   teacher's queued sheet never overrides saved marks. Receipts: append-only, both stand,
   overpayments flagged.
4. **Provisional receipts.** A payment taken offline is numbered `P-<PC>-<n>` at once, printed
   as provisional, and keeps that number on the real receipt.
5. **Drafts in the UI, for every form, encrypted.** Autosave works on any `<form>` without
   per-form code, sealed with Web Crypto AES-GCM under a non-extractable IndexedDB key.
6. **At rest.** The outbox and drafts are encrypted with audited primitives (`aes-gcm` crate,
   Web Crypto). Keys stay on the PC beside the data: this protects copies of the files, not
   against someone signed in as the same OS user.
7. **Things that need the server there and then** (sign-in, backup and restore, imports,
   setup, network and hub settings) are refused offline with a plain message, never queued.

## Consequences

- A change can arrive hours after it was made; the server receives the time it was made
  (`X-Madrasa-Queued-At`) and merges by it.
- The queue is per server (by certificate fingerprint), so a PC moved to another school never
  sends old changes to the wrong one.
- Sign-in cannot happen offline; a session that expired while away waits for the same person.
- Lists do not show queued additions until they are sent; the badge counts them.
- A server crash in the instant between saving a change and keeping its answer leaves the key
  "unknown" after ten minutes; the user is told to check before entering it again.
