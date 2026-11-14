#!/usr/bin/env bash
# Build the user manual as PDF, in English and Urdu, from docs/manual:
#
#   scripts/build-manual.sh [out-dir]     writes <out-dir>/Madrasa-User-Manual-EN.pdf and -UR.pdf
#                                         (default dist-manual/)
#
# Typesets docs/manual/manual.typ with the typst crates and fonts the workspace already has
# (crates/manual), so nothing else needs installing. The screenshots it uses are committed
# under docs/manual/screenshots; scripts/manual-screenshots.sh makes them again.
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-dist-manual}"
version=$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
cargo run --quiet --release -p madrasa-manual -- --root . --out "$out" --version "$version"
