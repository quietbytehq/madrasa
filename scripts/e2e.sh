#!/usr/bin/env bash
# Automated GUI tests (e2e/): the desktop front end, the parent app and the hub console in a
# real browser, against real servers and made-up data. Nothing here touches a school's data.
#
#   scripts/e2e.sh                         run everything
#   scripts/e2e.sh --project=desktop      one group (desktop, parent, console)
#   scripts/e2e.sh -g "voucher"            tests whose name matches
#   scripts/e2e.sh --keep                  start the servers and leave them running (then run
#                                          `pnpm exec playwright test -c e2e` yourself)
#
# Needs: PostgreSQL where this script may create and drop databases named e2e_* (the URL in
# MADRASA_E2E_DATABASE_URL, else MADRASA_TEST_DATABASE_URL), `psql`, `pdftotext` and `pdfinfo`
# (poppler), `unzip`,
# Rust, Node with `pnpm install` done, and Playwright's Chromium
# (`pnpm exec playwright install chromium`).
#
# What it runs, all on this PC:
#   - a school server on a small demo school: its API                127.0.0.1:47802
#     and the same API over TLS, as on a school network              127.0.0.1:47871
#   - a client's proxy to that server, with the offline queue        127.0.0.1:47803
#   - the Nexus hub with the demo network, serving its console       127.0.0.1:47880
#   - the desktop app and the parent app from Vite                   127.0.0.1:1425, :1445
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD

pg="${MADRASA_E2E_DATABASE_URL:-${MADRASA_TEST_DATABASE_URL:-}}"
if [ -z "$pg" ]; then
  echo "Set MADRASA_E2E_DATABASE_URL to a PostgreSQL server where e2e_* databases may be made." >&2
  exit 1
fi
for tool in psql pdftotext pdfinfo unzip curl; do
  command -v "$tool" >/dev/null || { echo "$tool is needed and was not found." >&2; exit 1; }
done
keep=""
args=()
while [ $# -gt 0 ]; do
  case "$1" in
    --keep) keep=1; shift ;;
    *) args+=("$1"); shift ;;
  esac
done

db_url() { echo "${pg%/*}/$1"; }
sql() { psql "$pg" -qAtc "$1" >/dev/null 2>&1; }
recreate() {
  sql "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '$1' AND pid <> pg_backend_pid()" || true
  sql "DROP DATABASE IF EXISTS $1"
  sql "CREATE DATABASE $1"
}

echo "== building"
export CARGO_INCREMENTAL=0
cargo build --quiet -p madrasa-api --bin madrasa-server -p madrasa-nexus --bin madrasa-nexus
bin="${CARGO_TARGET_DIR:-$root/target}/debug"
pnpm --silent --dir apps/desktop build:console >/dev/null

work="${E2E_WORK:-$(mktemp -d)}"
mkdir -p "$work"
pids=()
stop_all() {
  for p in "${pids[@]:-}"; do
    [ -n "$p" ] || continue
    kill -CONT "$p" 2>/dev/null || true # a test may have frozen it
    kill "$p" 2>/dev/null || true
  done
  pids=()
}
cleanup() { stop_all; [ -n "${E2E_WORK:-}" ] || rm -rf "$work"; }
trap cleanup EXIT

wait_for() {
  for _ in $(seq 1 240); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
  echo "$1 did not answer" >&2
  return 1
}
post() { curl -sf -X POST "$1" -H 'content-type: application/json' -d "$2"; }

SCHOOL=http://127.0.0.1:47802
CLIENT=http://127.0.0.1:47803
HUB=http://localhost:47880
PASSWORD=e2e-password-1

echo "== the school (a small demo school), as a server on a school network"
recreate e2e_school
mkdir -p "$work/school" "$work/client"
MADRASA_DATA_DIR="$work/school" MADRASA_DATABASE_URL=$(db_url e2e_school) \
  MADRASA_BIND=127.0.0.1:47802 MADRASA_LAN_BIND=127.0.0.1:47871 RUST_LOG=warn \
  "$bin/madrasa-server" >"$work/school.log" 2>&1 &
school_pid=$!
pids+=("$school_pid")
wait_for "$SCHOOL/api/v1/health"
post "$SCHOOL/api/v1/setup/demo" \
  "{\"admin_username\":\"admin\",\"admin_password\":\"$PASSWORD\",\"backup_password\":\"e2e-backup-password-1\",\"size\":\"small\"}" >/dev/null

echo "== a client PC's proxy to it"
MADRASA_DATA_DIR="$work/client" MADRASA_SERVER_URL=https://127.0.0.1:47871 \
  MADRASA_SERVER_FINGERPRINT="$(cat "$work/school/tls/fingerprint.txt")" \
  MADRASA_PROXY_BIND=127.0.0.1:47803 RUST_LOG=warn \
  "$bin/madrasa-server" proxy >"$work/client.log" 2>&1 &
pids+=($!)
wait_for "$CLIENT/api/v1/health"

echo "== the hub with the demo network"
recreate e2e_hub
NEXUS_DATABASE_URL=$(db_url e2e_hub) "$bin/madrasa-nexus" demo-network --yes >"$work/codes.txt"
code=$(grep -E "^ +[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4} +DEMO North Campus" "$work/codes.txt" | head -1 | awk '{print $1}')
NEXUS_DATABASE_URL=$(db_url e2e_hub) NEXUS_BIND=127.0.0.1:47880 NEXUS_PUBLIC_URL="$HUB" \
  NEXUS_SETUP_TOKEN=e2e-setup-token NEXUS_CONSOLE_DIR="$root/apps/desktop/dist-console" RUST_LOG=warn \
  "$bin/madrasa-nexus" >"$work/hub.log" 2>&1 &
pids+=($!)
wait_for "$HUB/api/nexus/v1/health"
post "$HUB/api/nexus/v1/setup" \
  "{\"setup_token\":\"e2e-setup-token\",\"hub_name\":\"E2E Head Office\",\"admin_username\":\"hub.admin\",\"admin_full_name\":\"Hub Administrator\",\"admin_password\":\"$PASSWORD\"}" >/dev/null
post "$HUB/api/nexus/v1/parent/activate" \
  "{\"code\":\"$code\",\"phone\":\"+923001234567\",\"password\":\"$PASSWORD\"}" >/dev/null

echo "== the apps"
# The package's own launcher, not npx: it becomes the server, so stopping it stops the server.
(cd apps/desktop && exec node_modules/.bin/vite --port 1425 --strictPort --host 127.0.0.1 >"$work/vite-desktop.log" 2>&1) &
pids+=($!)
(cd apps/parent && exec node_modules/.bin/vite --port 1445 --strictPort --host 127.0.0.1 >"$work/vite-parent.log" 2>&1) &
pids+=($!)
wait_for http://127.0.0.1:1425/
wait_for http://127.0.0.1:1445/

export E2E_WORK="$work" E2E_SCHOOL_PID="$school_pid" E2E_PASSWORD="$PASSWORD"
if [ -n "$keep" ]; then
  echo "Servers running (logs in $work). In another terminal:"
  echo "  E2E_WORK=$work E2E_SCHOOL_PID=$school_pid pnpm exec playwright test -c e2e"
  echo "Press Ctrl+C to stop."
  trap 'cleanup; exit 0' INT TERM
  wait
fi
echo "== tests"
pnpm exec playwright test -c e2e "${args[@]}"
