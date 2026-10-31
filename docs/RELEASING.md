# Releasing Madrasa Lite

Installers are built by GitHub Actions (`.github/workflows/release.yml`):

- **Tagged release**: push a tag `vX.Y.Z` on `main`. The workflow builds the Linux `.deb` and
  AppImage and the Windows NSIS installer with the bundled PostgreSQL, and creates a **draft
  pre-release** on GitHub with the files attached. Review the draft, edit the notes, publish.
- **Manual build**: run the "Release" workflow from the Actions tab (`workflow_dispatch`).
  It builds the same installers and uploads them as workflow artifacts (`madrasa-ubuntu-latest`,
  `madrasa-windows-latest`) without creating a release.

Every CI run on `main` and on `m1-*`/`dev/*` branches also builds the bundles and keeps them as
artifacts (GitHub's default retention), so a build of any merged stage is available for a while.

Before tagging:

1. `cargo test --workspace`, `pnpm --dir apps/desktop test`, lint and type checks are green
   (CI enforces this).
2. Bump `version` in the workspace `Cargo.toml`, `apps/desktop/package.json` and
   `apps/desktop/src-tauri/tauri.conf.json` together; the app shows the version under
   Settings and writes it into every backup file.
3. Update `docs/PROGRESS.md` with what the build contains and what to test by hand.

The Linux bundles are built on `ubuntu-22.04` so they run on Debian 12 and 13 and on Ubuntu
22.04 or newer (glibc 2.35); CI fails if the binary ever needs a newer glibc. Building on a
newer runner would silently raise that floor.

Installers are **unsigned** for now; Windows SmartScreen and Linux package tools warn about
that. Code signing is tracked for a later milestone.

## Pre-releases for testers (`vX.Y.Z-alpha`, `-beta`)

1. Merge what the build should contain into `main`; CI must be green there.
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

Tags and releases must be created by a person with push rights; the Claude Code cloud
session cannot create tags or releases (its GitHub proxy refuses those API paths), it can
only run the manual build (Actions → Release → Run workflow) that uploads the installers as
workflow artifacts.
