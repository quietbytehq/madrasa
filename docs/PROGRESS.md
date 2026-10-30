# Progress log

Running log of what was done, what is tested and what is left. Newest entries at the bottom
of each milestone section. See `docs/TASKS-M0.md` for the checklist.

## M0 — Foundation

### 2026-10-30 — Task 1: Repo
- Done: README (editions, status pre-alpha), CONTRIBUTING stub, `.gitignore`, `.editorconfig`,
  Cargo workspace (`crates/core`, `crates/db`, `crates/api`, `crates/demo-data`,
  `apps/desktop/src-tauri`), pnpm workspace (`apps/*`), `docs/spec.md` v0.1, ADR folder.
- Tested: `cargo check --workspace` passes with stub crates; pnpm workspace resolves.
- Left: everything else in M0.
