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

Installers are **unsigned** for now; Windows SmartScreen and Linux package tools warn about
that. Code signing is tracked for a later milestone.
