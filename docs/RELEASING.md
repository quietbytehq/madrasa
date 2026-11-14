# Releasing Madrasa Lite

Installers are built by GitHub Actions (`.github/workflows/release.yml`):

- **Tagged release**: push a tag `vX.Y.Z` on `main`. The workflow builds the Linux `.deb` and
  AppImage and the Windows NSIS installer with the bundled PostgreSQL, and creates a **draft
  pre-release** on GitHub with the files attached. Review the draft, edit the notes, publish.
- **Manual build**: run the "Release" workflow from the Actions tab (`workflow_dispatch`).
  It builds the same installers and uploads them as workflow artifacts (`madrasa-ubuntu-22.04`,
  `madrasa-windows-latest`) without creating a release.

Every CI run on `main` and on `m1-*`/`dev/*` branches also builds the bundles and keeps them as
artifacts (GitHub's default retention), so a build of any merged stage is available for a while.

Before tagging:

1. `cargo test --workspace`, `pnpm --dir apps/desktop test`, lint and type checks are green
   (CI enforces this).
2. Run `scripts/set-version.sh X.Y.Z` (the numeric part of the tag) and commit **all seven
   files it changes**: the workspace `Cargo.toml`, `Cargo.lock`, `package.json`, and
   `package.json` and `src-tauri/tauri.conf.json` of both `apps/desktop` and
   `apps/parent`. A unit test fails if the version files ever differ. `Cargo.lock` matters as much as the others: the
   Nexus hub image is built with `cargo build --locked`, which refuses a lock file that
   still names the old version (this is what broke the v0.4.0-alpha image). The script
   stops with an error if it cannot update the lock file; `scripts/check-lockfile.sh`
   checks it at any time.
   The Release workflow refuses a tag whose numeric part does not match `Cargo.toml` or
   whose `Cargo.lock` is out of date (its *Preflight* job, before anything is built), and
   passes the full tag (`0.1.3-alpha`) into the builds as `MADRASA_RELEASE`, which is what
   `/health`, `/system/features` and Help → About report. Builds without the variable
   report the plain Cargo version.
3. Update `docs/PROGRESS.md` with what the build contains and what to test by hand.

The Linux bundles are built on `ubuntu-22.04` so they run on Debian 12 and 13 and on Ubuntu
22.04 or newer (glibc 2.35); CI fails if the binary ever needs a newer glibc. Building on a
newer runner would silently raise that floor.
The `.deb` declares the libraries the program links (WebKitGTK 4.1, GTK 3, libsoup 3,
JavaScriptCore 4.1) and `libc6 (>= 2.35)` (`bundle.linux.deb.depends` in
`apps/desktop/src-tauri/tauri.conf.json`); CI installs the built package with apt in clean
Debian 12, Ubuntu 22.04 and Ubuntu 24.04 containers and fails if a library is missing. Every
other job runs on `ubuntu-24.04`, pinned rather than `ubuntu-latest`, so a runner image
change is a decision.

Installers are **unsigned** for now; Windows SmartScreen and Linux package tools warn about
that. Code signing is tracked for a later milestone.

## Pre-releases for testers (`vX.Y.Z-alpha`, `-beta`)

1. Merge what the build should contain into `main` **through a pull request, the version
   bump included**, and wait until CI is green on `main` before tagging. CI builds the
   Nexus hub image on every pull request and every push, the same way the Release workflow
   does, so a tag on a green commit will build. (v0.4.0-alpha was tagged a minute after a
   version bump pushed straight to `main`; CI on that commit failed in the image job, but
   the tag was already out.)
2. Write the notes in `docs/releases/<tag>.md` and update `TESTING.md` (the tester guide:
   install steps, sample data, tasks to try, how to report).
3. Tag `main` and push the tag; the Release workflow builds the installers and creates a
   **draft** pre-release with them attached:

   ```bash
   git tag -a v0.1.0-alpha main -m "Madrasa Lite v0.1.0-alpha"
   git push origin v0.1.0-alpha
   ```

4. On the draft release: paste the notes from `docs/releases/<tag>.md`, attach `TESTING.md`
   and the sample spreadsheets from `docs/testing/`, keep "pre-release" ticked, publish.

A tagged release also publishes the Nexus hub image to GitHub Container Registry as
`ghcr.io/quietbytehq/madrasa-nexus:<tag without v>` and `:latest`. If only the image job
fails, fix the cause, bump the version and tag again; do not move a tag that has been
pushed.

Tags and releases must be created by a person with push rights; the Claude Code cloud
session cannot create tags or releases (its GitHub proxy refuses those API paths), it can
only run the manual build (Actions → Release → Run workflow) that uploads the installers as
workflow artifacts.

## The parent app (Android)

The Release workflow also builds the parent app (`apps/parent`) as an Android package and
attaches it to the draft release: `madrasa-parent-<version>.apk` and a small
`…-SIGNING.txt` that says which key signed it. CI builds the same package on every pull
request and keeps it as the artifact `madrasa-parent-apk` (always with a test key), so a
change that would break the release build is seen before a release. A manual run of the
Release workflow builds the package and keeps it as an artifact.

The package holds the app for 64-bit and 32-bit ARM phones, Android 7.0 or newer.

### The signing key: needed before parents get the app

Android only installs signed packages, and only updates an app with a package signed by
**the same key**. Until a signing key is set up, the workflow signs with a throwaway key
made for that one build, names the file `…-testkey.apk` and says so in `SIGNING.txt` and
in the run's notices. Such a build is fine for trying the app. It must not go to parents:
the next release would have a different key, Android would refuse the update, and every
parent would have to remove the app and sign in again.

Set the key up once:

1. Make a keystore on a computer you trust (Java's `keytool`; choose a strong password
   when asked, and answer the name questions with the organisation's details):

   ```bash
   keytool -genkeypair -v -keystore madrasa-parent.p12 -storetype PKCS12 \
     -alias madrasa-parent -keyalg RSA -keysize 4096 -validity 10000
   ```

2. **Keep the file and its password safe, in two places, outside the repository.** If the
   key is lost, no update can ever be installed over the app parents already have. If it
   leaks, someone else can make packages that Android accepts as updates.
3. In the repository's settings (Secrets and variables → Actions) add four secrets:

   | Secret | Value |
   | --- | --- |
   | `ANDROID_KEYSTORE_BASE64` | The keystore file as one line of base64: `base64 -w0 madrasa-parent.p12` |
   | `ANDROID_KEYSTORE_PASSWORD` | The keystore's password |
   | `ANDROID_KEY_ALIAS` | `madrasa-parent` (the alias used above) |
   | `ANDROID_KEY_PASSWORD` | The key's password; leave it out when it is the same as the keystore's (it is, for a PKCS12 file made as above) |

4. Run the Release workflow by hand once and look at `SIGNING.txt` in the
   `madrasa-parent-apk` artifact: it must say "Signed with: the release key", and the file
   must have no `-testkey` in its name. Note the certificate's SHA-256: every later release
   must show the same one.

Nothing of the key is written to a log: the scripts pass passwords through the
environment, never on a command line, and the keystore is decoded into a temporary folder
that is removed when the step ends.

### Building it yourself

With the Android SDK and NDK, Java 17 and the Rust Android targets installed
(`rustup target add aarch64-linux-android armv7-linux-androideabi`):

```bash
pnpm install
scripts/build-parent-apk.sh
scripts/sign-apk.sh dist-apk/madrasa-parent-unsigned.apk dist-apk madrasa-parent-local
```

### Not done yet

- The package is offered as a file on the release page, to be installed by hand
  ("install unknown apps" must be allowed for the browser or file manager used). It is not
  on Google Play.
- No iOS build.
