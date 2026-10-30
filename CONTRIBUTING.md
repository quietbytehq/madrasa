# Contributing to Madrasa

Thanks for your interest. Madrasa is pre-alpha; the process below is deliberately light and
will grow with the project.

## Ground rules

- Read `CLAUDE.md` and `docs/spec.md` first. The non-negotiable rules in `CLAUDE.md` (UUID v7
  keys, `school_id`, sync columns, decimal money, audit log, backend permission checks, no real
  data, i18n, tests) apply to every change.
- Never commit real student, staff or fee data, credentials or secrets. Use the demo data
  generator for examples and tests.
- Every user-facing string goes through the translation layer (English and Urdu).
- Don't mark a task done until its tests pass.

## Workflow

1. Create a branch from `main`.
2. Keep commits small with clear, imperative messages.
3. Run the checks before pushing:
   ```bash
   pnpm lint && pnpm typecheck && pnpm test
   cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
   ```
4. Open a pull request. Describe what changed, how it was tested and any decisions worth an
   ADR (`docs/adr/`).

## Reporting issues

Open a GitHub issue with steps to reproduce. Never attach real school data.

## License

By contributing you agree that your contributions are licensed under the Apache License 2.0.
