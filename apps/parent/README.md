# Madrasa Parent

The parent app (alpha): a parent signs in, picks a child and sees attendance, results and
the report card, fee status and vouchers, the school's notices, and a conversation with the
class teacher. English and Urdu. Design: `docs/adr/0006-parent-app.md`; behaviour:
`docs/spec.md` §17.

- `src/` is the front end: Vite, React and TypeScript, built from the desktop app's design
  system. `@/…` imports are the desktop app's source (`apps/desktop/src`: components,
  tokens, translations); `@parent/…` is this app's own. Its strings are the `parent.*` keys
  of the shared translation files.
- `src-tauri/` is the Tauri 2 shell, built for Android. It only shows the front end: the
  app talks to a Madrasa Nexus hub over HTTPS and keeps nothing but the hub's address and
  the session on the phone.

## Try it in a browser

```bash
pnpm install
pnpm dev:parent        # http://localhost:1440
```

Open it at phone size (the browser's device toolbar). It needs a hub to talk to: for a
local one, see "Parent app" in `TESTING.md`. A plain `http://` hub address is accepted only
for this computer (`localhost`, `127.0.0.1`) and the Android emulator (`10.0.2.2`); any
other hub must be `https://`.

## Checks

```bash
pnpm --filter madrasa-parent lint
pnpm --filter madrasa-parent typecheck
pnpm --filter madrasa-parent test
pnpm build:parent
```

## Android

The Android project is generated, not kept in the repository (`src-tauri/gen` is ignored).
With the Android SDK and NDK, Java 17 and the Rust Android targets installed:

```bash
cd apps/parent
pnpm tauri android init      # once: writes src-tauri/gen/android
pnpm tauri android dev       # on an emulator or a connected phone
pnpm tauri android build --apk
```

Continuous integration builds the APK on every pull request and attaches it to releases;
see `docs/RELEASING.md`.
