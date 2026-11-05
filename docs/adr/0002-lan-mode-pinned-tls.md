# 0002 — LAN mode: loopback proxy with a pinned self-signed certificate
Date: 2026-11-05
Status: accepted

## Context
A K-12 school cannot have 60 teachers entering marks and attendance on one PC. Lite must let
several PCs on the school's local network work on one school's records, with no internet,
no domain, no IT staff and no purchased certificate. The webview (WebView2 / WebKitGTK)
refuses self-signed certificates and offers no per-app trust store, and schools cannot run a
private CA.

Options considered:
1. **Plain HTTP on the LAN.** Simplest, but passwords and session tokens would cross the
   school Wi-Fi in clear. Rejected.
2. **Self-signed certificate trusted by the webview.** Needs installing a CA certificate in
   each client's OS store, which is fragile across platforms and a security risk if the key
   leaks. Rejected.
3. **TLS with fingerprint pinning behind a loopback proxy** (chosen). The server creates one
   self-signed certificate at first start and shows its SHA-256 fingerprint. Each client runs
   a loopback HTTP proxy in its own process whose TLS client accepts only the certificate with
   the fingerprint confirmed by the user at setup. The webview keeps talking to
   `http://127.0.0.1:<port>` exactly as in standalone mode.

## Decision
Serve the same Axum router over TLS (`axum-server`, rustls with `ring`, `rcgen` certificate
stored in `<data_dir>/tls`) on `0.0.0.0:8471` in server mode, alongside the loopback listener
for the server's own UI. Clients keep no database; the desktop shell forwards every request
through `lan::serve_proxy` with a `PinnedVerifier`. Discovery is a UDP beacon on port 47121
answering `MADRASA_DISCOVER v1`; the fingerprint it returns is only a convenience and the user
must confirm it against the server's Settings → Network screen before it is saved. Logins,
sessions and permissions are unchanged: every client user authenticates with their own account
and the backend enforces permissions as on one PC.

## Consequences
- No change to the UI's API client or to any route; one proxy module carries LAN mode.
- A reinstalled server gets a new fingerprint and clients must be set up again (by design).
- The Windows firewall must allow Madrasa on private networks; the installer does not change
  firewall rules, so the mode page, Settings → Network and TESTING.md show the `netsh`
  command.
- Standard edition (a separate server process, several schools) can reuse the TLS listener and
  the pinned client without changing the wire protocol; a CA-signed certificate can replace
  pinning later by adding a second verifier.
