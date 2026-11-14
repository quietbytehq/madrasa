#!/usr/bin/env bash
# Make every screenshot in the user manual again, from the demo school and the demo network,
# in English and Urdu (docs/manual/screenshots/{en,ur}/*.webp).
#
#   scripts/manual-screenshots.sh                  both languages, every screenshot
#   scripts/manual-screenshots.sh --lang ur        one language
#   scripts/manual-screenshots.sh --only fees-pay,fees-receipt
#   scripts/manual-screenshots.sh --keep           leave the servers running afterwards
#
# Needs: PostgreSQL where this script may create and drop databases named manual_* (the URL
# in MADRASA_MANUAL_DATABASE_URL, else MADRASA_TEST_DATABASE_URL), `psql`, Rust, Node with
# `pnpm install` done, and Playwright's Chromium (`pnpm exec playwright install chromium`).
#
# What it runs, all on this PC and all made-up data:
#   - the school API on the demo school (Standard edition)        127.0.0.1:47702
#   - the school API on an empty database (the first-run screens)  127.0.0.1:47701
#   - the Nexus hub with the demo network, serving its console     127.0.0.1:47780
#   - a Nexus hub before its first setup                           127.0.0.1:47781
#   - the desktop app and the parent app from Vite                 127.0.0.1:1420, :1440
# Each language starts from fresh copies of the same databases, so both books show the same.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD

pg="${MADRASA_MANUAL_DATABASE_URL:-${MADRASA_TEST_DATABASE_URL:-}}"
if [ -z "$pg" ]; then
  echo "Set MADRASA_MANUAL_DATABASE_URL to a PostgreSQL server where manual_* databases may be made." >&2
  exit 1
fi
langs="en ur"
only=""
keep=""
while [ $# -gt 0 ]; do
  case "$1" in
    --lang) langs="$2"; shift 2 ;;
    --only) only="$2"; shift 2 ;;
    --keep) keep=1; shift ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done

db_url() { echo "${pg%/*}/$1"; }
sql() { psql "$pg" -qAtc "$1" >/dev/null; }
recreate() { # recreate <name> [template]
  sql "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '$1' AND pid <> pg_backend_pid()"
  sql "DROP DATABASE IF EXISTS $1"
  if [ -n "${2:-}" ]; then sql "CREATE DATABASE $1 TEMPLATE $2"; else sql "CREATE DATABASE $1"; fi
}

echo "== building"
export CARGO_INCREMENTAL=0
cargo build --quiet -p madrasa-api --bin madrasa-server -p madrasa-demo-data --bin madrasa-demo \
  -p madrasa-nexus --bin madrasa-nexus
bin="${CARGO_TARGET_DIR:-$root/target}/debug"
pnpm --silent --dir apps/desktop build:console >/dev/null

work=$(mktemp -d)
pids=()
stop_all() {
  for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done
  pids=()
  sleep 1
}
cleanup() { [ -z "$keep" ] && stop_all; rm -rf "$work"; }
trap cleanup EXIT

wait_for() { # wait_for <url>
  for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
  echo "$1 did not answer" >&2
  return 1
}

start_school() { # start_school <db> <port>
  MADRASA_DATABASE_URL=$(db_url "$1") MADRASA_BIND="127.0.0.1:$2" RUST_LOG=warn \
    "$bin/madrasa-server" >"$work/school-$2.log" 2>&1 &
  pids+=($!)
  wait_for "http://127.0.0.1:$2/api/v1/health"
}

start_hub() { # start_hub <db> <port>
  NEXUS_DATABASE_URL=$(db_url "$1") NEXUS_BIND="127.0.0.1:$2" \
    NEXUS_PUBLIC_URL="http://127.0.0.1:$2" NEXUS_SETUP_TOKEN="manual-setup-token" \
    NEXUS_CONSOLE_DIR="$root/apps/desktop/dist-console" RUST_LOG=warn \
    "$bin/madrasa-nexus" >"$work/hub-$2.log" 2>&1 &
  pids+=($!)
  wait_for "http://127.0.0.1:$2/api/nexus/v1/health"
}

echo "== the demo school, the demo network (once)"
recreate manual_school_tpl
MADRASA_DATABASE_URL=$(db_url manual_school_tpl) "$bin/madrasa-demo" load --yes >/dev/null
# Every module: the Standard edition.
psql "$(db_url manual_school_tpl)" -qAtc "UPDATE settings SET value = '\"standard\"' WHERE key = 'edition'" >/dev/null
recreate manual_hub_tpl
NEXUS_DATABASE_URL=$(db_url manual_hub_tpl) "$bin/madrasa-nexus" demo-network --yes >"$work/codes.txt"
code=$(grep -m1 "DEMO North Campus" "$work/codes.txt" | awk '{print $1}')
start_hub manual_hub_tpl 47780
curl -sf -X POST http://127.0.0.1:47780/api/nexus/v1/setup -H 'content-type: application/json' \
  -d '{"setup_token":"manual-setup-token","hub_name":"DEMO Head Office","admin_username":"demo.admin","admin_full_name":"DEMO Administrator","admin_password":"demo-password-1"}' >/dev/null
curl -sf -X POST http://127.0.0.1:47780/api/nexus/v1/parent/activate -H 'content-type: application/json' \
  -d "{\"code\":\"$code\",\"phone\":\"+923001234567\",\"password\":\"demo-password-1\"}" >/dev/null
stop_all

echo "== the apps"
(cd apps/desktop && exec npx vite --port 1420 --strictPort --host 127.0.0.1 >"$work/vite-desktop.log" 2>&1) &
pids_vite=($!)
(cd apps/parent && exec npx vite --port 1440 --strictPort --host 127.0.0.1 >"$work/vite-parent.log" 2>&1) &
pids_vite+=($!)
wait_for http://127.0.0.1:1420/
wait_for http://127.0.0.1:1440/

for lang in $langs; do
  echo "== $lang"
  recreate manual_school manual_school_tpl
  recreate manual_setup
  recreate manual_hub manual_hub_tpl
  recreate manual_hubsetup
  start_school manual_school 47702
  start_school manual_setup 47701
  start_hub manual_hub 47780
  start_hub manual_hubsetup 47781
  node docs/manual/tools/shoot.mjs --lang "$lang" --out docs/manual/screenshots \
    ${only:+--only "$only"}
  [ -n "$keep" ] && [ "$lang" = "${langs##* }" ] && break
  stop_all
done

if [ -n "$keep" ]; then
  echo "Servers still running (logs in $work). Press Ctrl+C to stop."
  trap 'stop_all; kill "${pids_vite[@]}" 2>/dev/null; rm -rf "$work"' INT TERM
  wait
fi
kill "${pids_vite[@]}" 2>/dev/null || true
echo "Screenshots written to docs/manual/screenshots."
