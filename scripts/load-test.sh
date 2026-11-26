#!/usr/bin/env bash
# Load test: a made-up school of about 5,000 students with three years of fees, attendance
# and marks (scripts/load/seed.sql), and the three promises checked against it:
#
#   - every main screen opens in under 1 second
#   - fee vouchers for the whole school are generated in under 60 seconds
#   - a backup is made in under 2 minutes, and its check (a restore into a temporary
#     database) passes
#   - after a busy fee day (300 payments) the day's lists still open in under 1 second
#
# plus, for the record, how long "Export everything" takes and how big it is, and how much
# memory the server needed for the backup and for the export.
#
#   scripts/load-test.sh            build, seed, measure, stop (exit 1 if a promise is broken)
#   scripts/load-test.sh --keep     leave the server and the database for a closer look
#
# Needs PostgreSQL where it may create and drop the database `madrasa_load` (URL in
# MADRASA_LOAD_DATABASE_URL, else MADRASA_TEST_DATABASE_URL), psql, Rust, Node with
# `pnpm install` done and Playwright's Chromium. Takes about ten minutes after the build.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
pg="${MADRASA_LOAD_DATABASE_URL:-${MADRASA_TEST_DATABASE_URL:-}}"
[ -n "$pg" ] || { echo "Set MADRASA_LOAD_DATABASE_URL to a PostgreSQL server." >&2; exit 1; }
keep=""; [ "${1:-}" = "--keep" ] && keep=1
url="${pg%/*}/madrasa_load"
API=http://127.0.0.1:47902
FRONT=http://127.0.0.1:1426
PASSWORD=load-password-1
work="${LOAD_WORK:-$(mktemp -d)}"; mkdir -p "$work"
out="$work/results.md"; : >"$out"
say() { echo "$*" | tee -a "$out"; }

echo "== building (release)"
export CARGO_INCREMENTAL=0
cargo build --release --quiet -p madrasa-api --bin madrasa-server
bin="${CARGO_TARGET_DIR:-$root/target}/release/madrasa-server"

pids=()
cleanup() {
  for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done
  if [ -z "$keep" ]; then
    psql "$pg" -qAtc "DROP DATABASE IF EXISTS madrasa_load WITH (FORCE)" >/dev/null 2>&1 || true
    [ -n "${LOAD_WORK:-}" ] || rm -rf "$work"
  fi
}
trap cleanup EXIT
wait_for() { for _ in $(seq 1 240); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done; echo "$1 did not answer" >&2; return 1; }
post() { curl -sf -X POST "$API/api/v1$1" -H 'content-type: application/json' ${token:+-H "authorization: Bearer $token"} -d "$2"; }
# Seconds a command takes, to one decimal.
timed() { local t0 t1; t0=$(date +%s.%N); "$@" >"$work/last.out" 2>"$work/last.err"; t1=$(date +%s.%N); echo "$t1 - $t0" | bc | xargs printf '%.1f'; }

echo "== the school"
psql "$pg" -qAtc "DROP DATABASE IF EXISTS madrasa_load WITH (FORCE)" -c "CREATE DATABASE madrasa_load" >/dev/null
MADRASA_DATA_DIR="$work/data" MADRASA_DATABASE_URL="$url" MADRASA_BIND=127.0.0.1:47902 RUST_LOG=warn \
  "$bin" >"$work/server.log" 2>&1 &
server=$!; pids+=("$server")
wait_for "$API/api/v1/health"
token=""
post /setup "{\"school_name\":\"Load Test School\",\"school_code\":\"LOAD\",\"academic_year_start_month\":4,\"admin_full_name\":\"Load Admin\",\"admin_username\":\"admin\",\"admin_password\":\"$PASSWORD\",\"backup_password\":\"load-backup-password-1\"}" >/dev/null
token=$(post /auth/login "{\"username\":\"admin\",\"password\":\"$PASSWORD\"}" | sed -E 's/.*"token":"([^"]+)".*/\1/')
post /academic-years '{"start_year":2026,"start_month":4,"activate":true}' >/dev/null
echo "== three years of records (a few minutes)"
psql "$url" -q -v ON_ERROR_STOP=1 -f scripts/load/seed.sql | tee "$work/seed.txt" | sed -n '/what/,$p'
say "## Load test, $(date -u +%Y-%m-%d), $(git rev-parse --short HEAD)"
say ""
say '```'
sed -n '/what/,$p' "$work/seed.txt" | sed '/^$/d' | tee -a "$out" >/dev/null
say "database size: $(psql "$url" -qAtc "SELECT pg_size_pretty(pg_database_size('madrasa_load'))")"
say '```'

echo "== the front end"
(cd apps/desktop && exec node_modules/.bin/vite --port 1426 --strictPort --host 127.0.0.1 >"$work/vite.log" 2>&1) &
pids+=($!)
wait_for "$FRONT/"

failed=0
echo "== screens"
node scripts/load/screens.mjs "$API" "$FRONT" "$PASSWORD" 1000 | tee -a "$out" || failed=1

echo "== fee vouchers for the whole school"
month=$(date -u +%Y-%m-01)
secs=$(timed post /fees/vouchers/generate "{\"month\":\"$month\"}")
made=$(sed -E 's/.*"created":([0-9]+).*/\1/' "$work/last.out")
say ""
say "| Operation | Took | Limit | Result |"
say "| --- | ---: | ---: | --- |"
verdict() { if [ "$(echo "$1 <= $2" | bc)" = 1 ]; then echo "within"; else echo "**over**"; failed=1; fi; }
v=$(verdict "$secs" 60); [ "$v" = within ] || failed=1
say "| Generate vouchers for $month ($made vouchers) | ${secs} s | 60 s | $v |"

echo "== a busy fee day: 300 payments at the counter"
ids=$(curl -sf "$API/api/v1/fees/vouchers?status=unpaid&limit=300" -H "authorization: Bearer $token" | grep -o '"id":"[^"]*"' | cut -d'"' -f4 | head -300)
t0=$(date +%s.%N); paid=0
for id in $ids; do post "/fees/vouchers/$id/payments" '{"amount":"500.00","method":"cash"}' >/dev/null && paid=$((paid + 1)); done
secs=$(echo "$(date +%s.%N) - $t0" | bc | xargs printf '%.1f')
say "| Take $paid payments, one after the other | ${secs} s | none set | $(echo "$secs * 1000 / $paid" | bc) ms each |"
# The lists that are long on such a day, asked as the screens ask them.
for path in /fees/collections /fees/defaulters /dashboard "/fees/vouchers?month=$month"; do
  slowest=0
  for _ in 1 2 3; do
    t=$(curl -sf -o "$work/last.out" -w '%{time_total}' "$API/api/v1$path" -H "authorization: Bearer $token")
    [ "$(echo "$t > $slowest" | bc)" = 1 ] && slowest=$t
  done
  v=$(verdict "$slowest" 1); [ "$v" = within ] || failed=1
  say "| After that day: $path ($(($(wc -c <"$work/last.out") / 1024)) KB) | $(printf '%.2f' "$slowest") s | 1 s | $v |"
done

echo "== backup"
# The server's memory is watched over the backup alone (the peak is reset first, where the
# system allows it).
echo 5 >"/proc/$server/clear_refs" 2>/dev/null && reset=1 || reset=""
curl -sf -X PUT "$API/api/v1/backup/settings" -H 'content-type: application/json' -H "authorization: Bearer $token" \
  -d "{\"enabled\":true,\"time\":\"02:00\",\"directory\":\"$work/backups\",\"keep\":3}" >/dev/null
secs=$(timed post /backup/run '{}')
size=$(ls -la "$work"/backups/*.mbk 2>/dev/null | awk '{s+=$5} END {printf "%.0f MB", s/1048576}')
v=$(verdict "$secs" 120); [ "$v" = within ] || failed=1
say "| Back up now (${size:-?}) | ${secs} s | 120 s | $v |"
# Its check (restore into a temporary database, count every table) follows on its own.
t0=$(date +%s.%N); check=""
for _ in $(seq 1 600); do
  check=$(curl -sf "$API/api/v1/backup/status" -H "authorization: Bearer $token" | sed -nE 's/.*"check":"([a-z]+)".*/\1/p')
  [ -n "$check" ] && break; sleep 1
done
secs=$(echo "$(date +%s.%N) - $t0" | bc | xargs printf '%.1f')
[ "$check" = verified ] || failed=1
say "| The backup's check: ${check:-did not finish} | ${secs} s | none set | $([ "$check" = verified ] && echo "restored and counted" || echo "**not verified**") |"
peak_backup=$(awk '/VmHWM/ {printf "%.0f MB", $2/1024}' "/proc/$server/status")

echo "== export everything"
echo 5 >"/proc/$server/clear_refs" 2>/dev/null || true
t0=$(date +%s.%N)
curl -sf "$API/api/v1/export/everything" -H "authorization: Bearer $token" -o "$work/everything.xlsx"
secs=$(echo "$(date +%s.%N) - $t0" | bc | xargs printf '%.1f')
say "| Export everything ($(du -m "$work/everything.xlsx" | cut -f1) MB workbook) | ${secs} s | none set | for the record |"
say ""
if [ -n "$reset" ]; then
  say "Server memory: at most $peak_backup during the backup and its check, at most $(awk '/VmHWM/ {printf "%.0f MB", $2/1024}' "/proc/$server/status") during the export, $(awk '/VmRSS/ {printf "%.0f MB", $2/1024}' "/proc/$server/status") at the end."
else
  say "Server memory: at most $(awk '/VmHWM/ {printf "%.0f MB", $2/1024}' "/proc/$server/status") over the whole run, $(awk '/VmRSS/ {printf "%.0f MB", $2/1024}' "/proc/$server/status") at the end."
fi

cp "$out" "$root/target/load-results.md" 2>/dev/null || true
echo
echo "Results: $out (copied to target/load-results.md)"
if [ -n "$keep" ]; then echo "Server left running at $API (database madrasa_load). Ctrl+C to stop."; wait "$server"; fi
exit $failed
