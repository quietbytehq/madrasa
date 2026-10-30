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

### 2026-10-30 — Task 2: App skeleton
- Done: `apps/desktop` Tauri 2 shell (`src-tauri`, icons generated from `assets`-free source
  via `tauri icon`), React 19 + TypeScript (strict) + Vite 7 + Tailwind 3.4. Scripts: `lint`
  (eslint 9 flat config, react-hooks, typescript-eslint), `format`/`format:check` (prettier +
  tailwind plugin), `typecheck` (`tsc -b`), `test` (vitest + Testing Library, jsdom). Rust:
  `rustfmt.toml`, clippy with `-D warnings` clean.
- Tested: `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test`, `pnpm build`,
  `cargo build -p madrasa-desktop`, `pnpm tauri build --debug --no-bundle`; the debug binary
  launches under Xvfb and runs until killed (only harmless libEGL warnings).
- Left: in-process API start-up in the shell (task 5), bundling targets in CI (task 10).

### 2026-10-30 — Task 3: Design system
- Done: tokens in `src/styles/tokens.css` (light, dark, print-forces-light) exposed through
  `tailwind.config.ts`; `ThemeProvider` (system / light / dark, persisted). Inter and Noto
  Nastaliq Urdu bundled via `@fontsource` (woff2 shipped in `dist`, no network). Components in
  `src/components/ui`: Button, Input, Select, Table (sort, filter, paginate), Card, Badge,
  Dialog (focus trap, Escape, restore focus), Toast (provider + `useToast`, aria-live),
  EmptyState, Spinner, Breadcrumbs. i18n layer `src/i18n` with `en` and `ur` dictionaries,
  `t()` with placeholders, `Intl` number/date formatting, `<html lang dir>` switching.
- Tested: 28 vitest tests: dictionary parity (every key, same placeholders), provider
  behaviour, every component, and an RTL suite that renders all components under Urdu and
  asserts `dir="rtl"`, translated chrome and no physical left/right utilities.
- Left: nothing for M0.

### 2026-10-30 — Task 4: App shell
- Done: `AppShell` (skip link, collapsible sidebar, top bar, main region), `Sidebar` driven by
  `src/app/navigation.ts` and edition flags (`FeaturesProvider` reads `/system/features`,
  Lite defaults until the API answers), `TopBar` with Ctrl+K search stub, notifications stub,
  help dialog, user menu (language, theme, logout). `SessionProvider` (token in localStorage,
  `/auth/me` on start), `RequireAuth`, `SetupGate` (routes to `/setup` when the API says
  setup is needed), `Dashboard` with four placeholder cards and an empty activity list,
  `ComingSoon` page for every unfinished module, `Login` page. Typed API client in `src/api`.
- Tested: 8 shell tests (edition-based visibility for Lite vs Nexus, dashboard placeholders,
  Coming soon navigation, Ctrl+K, language switch + logout, collapse + keyboard order,
  anonymous redirect). Playwright screenshots of the built UI at 1366×768 and 1920×1080 in
  en/ur × light/dark: no console errors, no horizontal scroll, RTL mirrored correctly.
- Left: real setup wizard page (task 8); login against the real API (task 6).
