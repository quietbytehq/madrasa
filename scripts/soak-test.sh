#!/usr/bin/env bash
# Long-run test: a school server used for hours by people and client PCs that come and go,
# while its memory, its open files and its database processes are watched
# (scripts/load/soak.mjs says exactly what is done and what is checked).
#
#   scripts/soak-test.sh              8 hours
#   scripts/soak-test.sh 20           20 minutes (trends are judged from 30 minutes up)
#
# By default the server runs its bundled PostgreSQL, as an all-in-one install does: that is
# the only way to check that no database process is left behind. The bundled PostgreSQL
# refuses to run as root, so run this as an ordinary user. With SOAK_DATABASE_URL set, an
# outside PostgreSQL database (empty, made by you) is used instead and that check is skipped.
#
# SOAK_BIN       a madrasa-server already built (else it is built here)
# SOAK_PROFILE   release (default) or dev
# SOAK_WORK      where the logs, samples.csv and results.md go (else a temporary folder)
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
minutes="${1:-480}"
if [ -z "${SOAK_DATABASE_URL:-}" ] && [ "$(id -u)" = 0 ]; then
  echo "The bundled PostgreSQL does not run as root. Run as an ordinary user, or set SOAK_DATABASE_URL." >&2
  exit 1
fi
bin="${SOAK_BIN:-}"
if [ -z "$bin" ]; then
  echo "== building"
  export CARGO_INCREMENTAL=0
  feature=(); [ -n "${SOAK_DATABASE_URL:-}" ] || feature=(--features embedded-postgres)
  if [ "${SOAK_PROFILE:-release}" = dev ]; then
    cargo build --quiet -p madrasa-api "${feature[@]}" --bin madrasa-server
    bin="${CARGO_TARGET_DIR:-$root/target}/debug/madrasa-server"
  else
    cargo build --release --quiet -p madrasa-api "${feature[@]}" --bin madrasa-server
    bin="${CARGO_TARGET_DIR:-$root/target}/release/madrasa-server"
  fi
fi
work="${SOAK_WORK:-$(mktemp -d)}"
mkdir -p "$work"
echo "== running for $minutes minutes (logs and results in $work)"
status=0
SOAK_BIN="$bin" SOAK_WORK="$work" SOAK_MINUTES="$minutes" node scripts/load/soak.mjs || status=$?
mkdir -p "$root/target" 2>/dev/null && cp "$work/results.md" "$root/target/soak-results.md" 2>/dev/null || true
exit "$status"
