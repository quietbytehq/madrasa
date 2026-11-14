<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/madrasa-logo-dark.svg">
    <img src="assets/brand/madrasa-logo.svg" alt="Madrasa: School Information System" width="440">
  </picture>
</p>

# Madrasa

**Madrasa** is a free, open-source desktop School Information System (SIS) for schools
that need reliable, offline-capable administration: students, attendance, examinations,
fees and finance, teachers, reports and backups.

Status: **alpha** (v0.4.x-alpha). Testers are trying it with made-up data; it is not yet
ready for a school's real records. Every release tag ends in `-alpha`.

## Download

All builds are on the [releases page](https://github.com/quietbytehq/madrasa/releases)
(newest first; they are marked as pre-releases, so GitHub's "latest release" link does not
show them). Each release has:

| File | For |
| --- | --- |
| `Madrasa_<version>_x64-setup.exe` | Windows 10 and 11, 64-bit |
| `Madrasa_<version>_amd64.deb` | Debian 12 and 13, Ubuntu 22.04 or newer, Linux Mint 21 or newer |
| `Madrasa_<version>_amd64.AppImage` | Other 64-bit Linux from 2022 or later (glibc 2.35) |
| `madrasa-parent-<version>-alpha….apk` | The parent app for Android 7.0 or newer (see below) |
| `ghcr.io/quietbytehq/madrasa-nexus:<version>-alpha` | The Madrasa Nexus hub, as a container image (also `:latest`) |

The installers are not signed yet: Windows shows "Windows protected your PC" (choose *More
info* → *Run anyway*). How to install and what to try: [TESTING.md](TESTING.md).

### Installing the .deb

Install it with `apt`, which also installs the libraries it needs (WebKitGTK 4.1 and GTK 3)
when they are missing:

```bash
sudo apt install ./Madrasa_0.4.3_amd64.deb
```

Note the `./`: without it apt looks for a package of that name online. `dpkg -i` does not
install dependencies; if you used it and it stopped with "dependency problems", run
`sudo apt -f install` to finish. The package needs `libwebkit2gtk-4.1-0`, which Debian 11 and
Ubuntu 20.04 do not have; on those, use the AppImage.

## Editions

| Edition | Scope | Status |
| --- | --- | --- |
| **Lite** | One school, up to ~1,200 students and ~250 staff accounts (soft limits): students, attendance, examinations and report cards, fees, teachers, reports, backups. All-in-one desktop install with a bundled database. | Alpha |
| **Standard** | Everything in Lite plus timetable, leave, admissions, library, transport, hostel, inventory, directory (Active Directory) login and a read-only API; one server PC and many client PCs. Switched on in Settings → Edition. | Alpha |
| **Madrasa Nexus** | A hub that joins several schools into one network under a head office, like an Active Directory forest root. Each school keeps working on its own install and syncs a copy to the hub every few minutes; head office gets network-wide reports, can correct records and can hold schools' backup keys. Any edition can join. | Alpha: hub server and web console (`crates/nexus`); container image on GitHub Container Registry; [deployment guide](docs/nexus/deploy.md) for Docker Compose, Azure and Google Cloud |
| **Parent app** | Android app for parents of a school that has joined a hub: their own children's attendance, approved results and report card, fees and vouchers, school notices, and messages with the class teacher. English and Urdu. | Alpha (`apps/parent`). Builds so far are signed with a test key; see [releasing](docs/RELEASING.md) |

All of it is free and open source (Apache 2.0), in English and Urdu.

## Stack

- Desktop shell: [Tauri 2](https://tauri.app) (Rust)
- UI: React + TypeScript + Vite + Tailwind CSS, Lucide icons, Recharts
- Backend: Rust + Axum REST API under `/api/v1`
- Database: PostgreSQL with `sqlx` migrations (bundled for all-in-one installs)
- Crypto: `argon2` (Argon2id) and `aes-gcm` only

## Repository layout

```
apps/desktop/        Tauri app (src-tauri/ Rust shell, src/ React UI)
apps/parent/         Parent app for Android, alpha (src-tauri/ Tauri 2 mobile shell, src/ React UI)
crates/api/          Axum API server
crates/core/         domain logic (fees, grading, permissions): pure, well tested
crates/db/           migrations, queries
crates/demo-data/    fake data generator
crates/nexus/        Madrasa Nexus hub server
docs/                spec.md, task lists, progress log, architecture decisions (docs/adr/)
```

## Getting started (developers)

Prerequisites: Rust (stable), Node 22, pnpm, PostgreSQL 16 for running the tests, and the
[Tauri 2 Linux/Windows prerequisites](https://tauri.app/start/prerequisites/).

```bash
pnpm install
cargo build --workspace
pnpm --filter madrasa-desktop tauri dev
```

Database-backed tests need a PostgreSQL server. Set `MADRASA_TEST_DATABASE_URL` (for example
`postgres://postgres:postgres@localhost:5432/madrasa_test`) before running `cargo test`;
without it those tests are skipped.

To try the app with fake data, load the demo school into a database (never run this against a
real school's database; it refuses if one is present):

```bash
MADRASA_DATABASE_URL=postgres://postgres:postgres@localhost:5432/madrasa cargo run -p madrasa-demo-data --bin madrasa-demo -- load --yes
```

Continuous integration (`.github/workflows/ci.yml`) runs lint, type checks, tests and the
desktop, hub image and Android builds on every pull request; tagged `v*-alpha` releases
publish the unsigned installers, the parent app's APK and the hub image
(`docs/RELEASING.md`).

See `docs/spec.md` for the product specification, `docs/user-guide/` for the user guide
(English and Urdu) and `docs/PROGRESS.md` for what has actually been built and tested.

## License

Apache License 2.0. See `LICENSE`.
