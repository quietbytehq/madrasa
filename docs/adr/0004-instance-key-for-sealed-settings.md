# ADR 0004: An instance key seals small secrets kept in settings

## Status

Accepted (Standard stage 8).

## Context

Directory login needs a bind account password that the server uses on every login. It
cannot be hashed (it must be sent to the directory) and the settings table is the only
store shared by every mode (single PC, LAN server, later the Standard server). Backups
are already encrypted end to end, so the goal is narrower: the password must not sit in
clear text in the settings table, in settings exports or in the audit log.

## Decision

- `madrasa_core::secrets` seals values with AES-256-GCM (random nonce, base64
  `nonce || ciphertext`), using the audited `aes-gcm` crate.
- The key is `instance.key`: 32 random bytes, global (no school), created on first use
  and never logged (`settings::REDACTED_KEYS`). Sealed values are stored under their own
  keys (`directory.bind_password`), also redacted.
- The API unseals on demand; the UI only learns whether a password is stored.

## Consequences

- Someone with full read access to the database can read both the key and the sealed
  value. This matches the threat model of a school server where the database is the
  trust boundary; it protects against casual exposure (audit log, exports, screens).
- Moving to a key kept outside the database (a key file or OS key store) is a matter of
  changing where `settings::instance_key` reads from; sealed values need re-sealing then.
