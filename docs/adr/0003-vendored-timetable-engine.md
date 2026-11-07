# 0003 — Vendor the timewizer scheduling engine into madrasa-core
Date: 2026-11-07
Status: accepted

## Context
The Standard timetable module needs a solver for school timetables (periods, breaks,
shifts, joint classes, teacher limits, availability grids, rooms), manual editing with
collision checks and substitution planning. quietbytehq/timewizer (Apache-2.0, same owner)
already has a tested engine built for exactly these schools: compile, difficulty order,
ejection chains, repair, relaxation, verify, plus edit, check and substitutions modules.
It works on its own string ids and has no database.

## Decision
Copy the engine's pure modules into `crates/core/src/timetable` (solver, model, lessons,
links, settings, check, substitutions, CSV importer) rather than depend on the timewizer
crate or rewrite the algorithm. Only module paths changed; the xlsx importer and the
desktop-only tests were dropped. The module is excluded from rustfmt and carries a few
clippy allows so later fixes from timewizer can be applied as plain diffs. Madrasa maps its
entities onto the engine in `crates/db::timetable::setup_for_year`: sections are the
engine's classes, staff its teachers, UUIDs are passed as strings; results come back as
`timetable_cards` rows. Attribution is kept in the module header under the Apache-2.0
licence terms.

## Consequences
- One solver for both products; improvements can be carried across by diff.
- The engine's JSON stays camelCase where the API exposes it directly (check findings,
  slot options, cover days, the generator report); Madrasa's own rows stay snake_case.
- A later move to a shared crate is possible without changing Madrasa's tables.
