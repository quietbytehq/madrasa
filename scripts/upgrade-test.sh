#!/usr/bin/env bash
# Upgrade test: a school that runs 0.4.1 with data in it is upgraded in place to this build,
# and nothing may be lost.
#
#   1. The 0.4.1 server (built from its tag) sets up the demo school, and then a person's
#      work is added through its API: a new student, a fee payment, an attendance sheet,
#      a backup.
#   2. That server is stopped and a copy of its database is kept aside.
#   3. This build's server is started on the same database and data folder: it upgrades the
#      database, as it does on a school's PC after the new installer has run.
#   4. Every row of every table of the old database must still be there with the same
#      values (new columns and new tables are fine); the person's work is read back through
#      the new API; the same administrator password still signs in; a 0.4.1 backup is
#      recognised and refused with a clear message instead of being restored wrongly.
#
# Needs PostgreSQL where it may create and drop the databases `madrasa_upgrade` and
# `madrasa_upgrade_before` (URL in MADRASA_UPGRADE_DATABASE_URL, else
# MADRASA_TEST_DATABASE_URL), psql, jq, git with the old tag, Rust.
#
#   UPGRADE_FROM         the tag to upgrade from (default v0.4.1-alpha)
#   UPGRADE_OLD_TARGET   where the old version is built (default target/upgrade-old)
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
from="${UPGRADE_FROM:-v0.4.1-alpha}"
pg="${MADRASA_UPGRADE_DATABASE_URL:-${MADRASA_TEST_DATABASE_URL:-}}"
[ -n "$pg" ] || { echo "Set MADRASA_UPGRADE_DATABASE_URL to a PostgreSQL server." >&2; exit 1; }
for tool in psql jq curl git; do
  command -v "$tool" >/dev/null || { echo "$tool is needed and was not found." >&2; exit 1; }
done
url="${pg%/*}/madrasa_upgrade"
before="${pg%/*}/madrasa_upgrade_before"
API=http://127.0.0.1:47922
PASSWORD=upgrade-password-1
BACKUP_PASSWORD=upgrade-backup-password-1
work="$(mktemp -d)"
pids=()
cleanup() {
  for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done
  psql "$pg" -qAtc "DROP DATABASE IF EXISTS madrasa_upgrade WITH (FORCE)" -c "DROP DATABASE IF EXISTS madrasa_upgrade_before WITH (FORCE)" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT
fail() { echo "FAILED: $*" >&2; exit 1; }
wait_for() { for _ in $(seq 1 240); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done; echo "$1 did not answer" >&2; return 1; }
token=""
api() { # api METHOD PATH [JSON]: prints the answer; fails on an error status.
  local method=$1 path=$2 body=${3:-}
  curl -sf -X "$method" "$API/api/v1$path" ${token:+-H "authorization: Bearer $token"} \
    ${body:+-H 'content-type: application/json' -d "$body"} || fail "$method $path"
}
status_of() { # status_of METHOD PATH JSON: prints "<status> <body>".
  curl -s -o "$work/answer" -w '%{http_code}' -X "$1" "$API/api/v1$2" -H "authorization: Bearer $token" -H 'content-type: application/json' -d "$3"
  echo " $(cat "$work/answer")"
}
start() { # start BINARY
  MADRASA_DATA_DIR="$work/data" MADRASA_DATABASE_URL="$url" MADRASA_BIND=127.0.0.1:47922 RUST_LOG=warn \
    "$1" >>"$work/server.log" 2>&1 &
  server=$!; pids=("$server")
  wait_for "$API/api/v1/health" || { tail -30 "$work/server.log" >&2; fail "the server did not start"; }
}
stop() { kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; pids=(); }

echo "== building $from and this build"
git rev-parse -q --verify "refs/tags/$from" >/dev/null || git fetch --quiet --depth 1 origin "refs/tags/$from:refs/tags/$from"
mkdir -p "$work/old"
git archive "$from" | tar -x -C "$work/old"
export CARGO_INCREMENTAL=0
old_target="${UPGRADE_OLD_TARGET:-$root/target/upgrade-old}"
(cd "$work/old" && CARGO_TARGET_DIR="$old_target" cargo build --quiet --locked -p madrasa-api --bin madrasa-server)
cargo build --quiet -p madrasa-api --bin madrasa-server
old_bin="$old_target/debug/madrasa-server"
new_bin="${CARGO_TARGET_DIR:-$root/target}/debug/madrasa-server"
old_version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$work/old/Cargo.toml" | head -1)
new_version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

echo "== a school on $old_version"
psql "$pg" -qAtc "DROP DATABASE IF EXISTS madrasa_upgrade WITH (FORCE)" -c "DROP DATABASE IF EXISTS madrasa_upgrade_before WITH (FORCE)" -c "CREATE DATABASE madrasa_upgrade" >/dev/null
start "$old_bin"
[ "$(api GET /health | jq -r .version)" = "$old_version" ] || fail "the old server does not say it is $old_version"
api POST /setup/demo "{\"admin_username\":\"admin\",\"admin_password\":\"$PASSWORD\",\"backup_password\":\"$BACKUP_PASSWORD\",\"size\":\"small\"}" >/dev/null
token=$(api POST /auth/login "{\"username\":\"admin\",\"password\":\"$PASSWORD\"}" | jq -r .token)

echo "== a person's work on $old_version"
section=$(api GET /sections | jq -r '.[0].id')
student=$(api POST /students "{\"full_name\":\"Upgrade Test Student\",\"father_name\":\"Upgrade Test Father\",\"gender\":\"male\",\"date_of_birth\":\"2015-03-04\",\"admission_date\":\"$(date +%F)\",\"section_id\":\"$section\"}")
student_id=$(echo "$student" | jq -r '.id // .student.id')
student_number=$(echo "$student" | jq -r '.student_number // .student.student_number')
[ -n "$student_id" ] && [ "$student_id" != null ] || fail "the old server did not create the student: $student"
voucher=$(api GET "/fees/vouchers?status=unpaid&limit=1" | jq -c '.[0]')
voucher_id=$(echo "$voucher" | jq -r .id)
receipt=$(api POST "/fees/vouchers/$voucher_id/payments" '{"amount":"125.00","method":"cash","note":"before the upgrade"}' | jq -c .receipt)
receipt_id=$(echo "$receipt" | jq -r .id)
receipt_no=$(echo "$receipt" | jq -r .receipt_no)
ledger_student=$(echo "$voucher" | jq -r .student_id)
day=""
for back in 0 1 2 3 4 5 6 7 8 9; do
  d=$(date -d "-$back day" +%F)
  if [ "$(api GET "/attendance/sheet?section_id=$section&date=$d" | jq -r .can_edit)" = true ]; then day=$d; break; fi
done
[ -n "$day" ] || fail "no school day in the last ten days"
absent=$(api GET "/attendance/sheet?section_id=$section&date=$day" | jq -r '.students[0].student_id')
api PUT /attendance/sheet "{\"section_id\":\"$section\",\"date\":\"$day\",\"entries\":[{\"student_id\":\"$absent\",\"status\":\"absent\",\"note\":\"before the upgrade\"}]}" >/dev/null
# The daily late-fee pass, which a server makes soon after it starts: made here by hand, so
# that the new version finds it done and what it changes afterwards is the upgrade's doing.
api POST /fees/late-fees/run '{}' >/dev/null
mkdir -p "$work/backups"
api PUT /backup/settings "{\"enabled\":false,\"time\":\"02:00\",\"directory\":\"$work/backups\",\"keep\":5}" >/dev/null
api POST /backup/run '{}' >/dev/null
old_backup=$(ls "$work/backups"/*.mbk | head -1)
# What the old version says about the school, to compare with what the new one says.
api GET "/students?page_size=1" | jq .total >"$work/old.students"
api GET "/fees/students/$ledger_student/ledger" | jq -S . >"$work/old.ledger"
api GET /fees/defaulters | jq -S . >"$work/old.defaulters"
stop

echo "== the database as $old_version left it is kept aside"
psql "$pg" -qAtc "CREATE DATABASE madrasa_upgrade_before TEMPLATE madrasa_upgrade" >/dev/null
old_migrations=$(psql "$before" -qAtc "SELECT count(*) FROM _sqlx_migrations")

echo "== upgrade: $new_version starts on the same database and data folder"
start "$new_bin"
[ "$(api GET /health | jq -r .version)" = "$new_version" ] || fail "the new server does not say it is $new_version"
new_migrations=$(psql "$url" -qAtc "SELECT count(*) FROM _sqlx_migrations WHERE success")
[ "$new_migrations" -gt "$old_migrations" ] || fail "no migration ran ($old_migrations before, $new_migrations after)"
echo "   database steps: $old_migrations -> $new_migrations"

echo "== every row of every table is still there"
# A row counts as kept when a row with the same values in the columns the old version had
# exists after the upgrade. Listed here, with the reason, is everything the upgrade is
# allowed to change; anything else fails the test.
#   settings: the new version stores its own values under existing keys (compared by key).
#   sessions: sign-ins end with a restart by design.
allowed_tables="sessions"
lost=0
tables=$(psql "$before" -qAtc "SELECT table_name FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE' AND table_name <> '_sqlx_migrations' ORDER BY 1")
checked=0; rows_checked=0
for t in $tables; do
  case " $allowed_tables " in *" $t "*) continue ;; esac
  exists=$(psql "$url" -qAtc "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_name='$t'")
  if [ "$exists" != 1 ]; then echo "   LOST: table $t is gone"; lost=$((lost + 1)); continue; fi
  old_cols=$(psql "$before" -qAtc "SELECT column_name FROM information_schema.columns WHERE table_schema='public' AND table_name='$t' ORDER BY ordinal_position")
  new_cols=$(psql "$url" -qAtc "SELECT column_name FROM information_schema.columns WHERE table_schema='public' AND table_name='$t' ORDER BY ordinal_position")
  cols=""
  # (A here-string, not a pipe: with pipefail, grep -q ending early fails the pipe now and then.)
  for c in $old_cols; do
    if grep -qx "$c" <<<"$new_cols"; then cols="$cols${cols:+,}\"$c\""; else echo "   LOST: column $t.$c is gone"; lost=$((lost + 1)); fi
  done
  [ -n "$cols" ] || continue
  query="SELECT md5(ROW($cols)::text) FROM \"$t\" ORDER BY 1"
  [ "$t" = settings ] && query="SELECT key FROM settings ORDER BY 1"
  psql "$before" -qAtc "$query" >"$work/rows.before"
  psql "$url" -qAtc "$query" >"$work/rows.after"
  missing=$(comm -23 "$work/rows.before" "$work/rows.after" | wc -l)
  n=$(wc -l <"$work/rows.before")
  if [ "$missing" != 0 ]; then
    echo "   LOST: $t: $missing of $n rows are gone or changed"; lost=$((lost + 1))
    # Show the first two, column by column, so the cause can be read off the log.
    if grep -qx id <<<"$old_cols"; then
      psql "$before" -qAtc "SELECT md5(ROW($cols)::text) || ' ' || id FROM \"$t\"" | sort >"$work/ids.before"
      for id in $(comm -23 "$work/rows.before" "$work/rows.after" | head -2 | while read -r h; do grep "^$h " "$work/ids.before" | cut -d' ' -f2; done); do
        psql "$before" -qAtxc "SELECT $cols FROM \"$t\" WHERE id = '$id'" >"$work/row.before"
        psql "$url" -qAtxc "SELECT $cols FROM \"$t\" WHERE id = '$id'" >"$work/row.after"
        if [ -s "$work/row.after" ]; then echo "     $id changed (before | after):"; diff "$work/row.before" "$work/row.after" | grep '^[<>]' | sed 's/^/       /'
        else echo "     $id is gone"; fi
      done
    fi
  fi
  checked=$((checked + 1)); rows_checked=$((rows_checked + n))
done
[ "$lost" = 0 ] || fail "$lost differences between the database before and after the upgrade"
echo "   $checked tables, $rows_checked rows: all there, unchanged"

echo "== the person's work, read through $new_version"
token=$(api POST /auth/login "{\"username\":\"admin\",\"password\":\"$PASSWORD\"}" | jq -r .token)
[ -n "$token" ] && [ "$token" != null ] || fail "the administrator's password no longer signs in"
[ "$(api GET "/students?page_size=1" | jq .total)" = "$(cat "$work/old.students")" ] || fail "the number of students changed"
[ "$(api GET "/students/$student_id" | jq -r '.student_number // .student.student_number')" = "$student_number" ] || fail "the student added before the upgrade is not as it was"
got=$(api GET "/fees/receipts/$receipt_id" | jq -r '(.receipt // .) | "\(.receipt_no) \(.amount)"')
[ "$got" = "$receipt_no 125.00" ] || fail "the receipt taken before the upgrade reads '$got', not '$receipt_no 125.00'"
got=$(api GET "/attendance/sheet?section_id=$section&date=$day" | jq -r --arg s "$absent" '.students[] | select(.student_id == $s) | "\(.status) \(.note)"')
[ "$got" = "absent before the upgrade" ] || fail "the attendance marked before the upgrade reads '$got'"
# Every figure the old version showed for a fee ledger and the defaulters list is still
# shown, with the same value (the new version may show more).
for what in ledger defaulters; do
  case $what in ledger) path="/fees/students/$ledger_student/ledger" ;; *) path=/fees/defaulters ;; esac
  api GET "$path" | jq -S . >"$work/new.$what"
  jq -n --slurpfile old "$work/old.$what" --slurpfile new "$work/new.$what" '
    def kept(o; n): if (o | type) == "object" then (n | type) == "object" and all(o | keys[]; . as $k | kept(o[$k]; n[$k]))
      elif (o | type) == "array" then (n | type) == "array" and (o | length) == (n | length) and all(range(0; o | length); . as $i | kept(o[$i]; n[$i]))
      else o == n end;
    kept($old[0]; $new[0])' | grep -qx true || { diff "$work/old.$what" "$work/new.$what" | head -20 >&2; fail "the $what differs after the upgrade"; }
done
echo "   student, receipt $receipt_no, attendance, fee ledger, defaulters: as before"

echo "== a backup made by $old_version"
# Restoring it needs the version that made it (the tables differ). What matters here is
# that the new version says so in words and changes nothing.
count_before=$(psql "$url" -qAtc "SELECT count(*) FROM students")
answer=$(status_of POST /backup/restore "{\"path\":\"$old_backup\",\"password\":\"$BACKUP_PASSWORD\"}")
case "$answer" in
  409*"different version"*) echo "   refused with: $(echo "${answer#* }" | jq -r '.message // .error.message // .error')" ;;
  *) fail "restoring a $old_version backup answered: $answer" ;;
esac
[ "$(psql "$url" -qAtc "SELECT count(*) FROM students")" = "$count_before" ] || fail "the refused restore changed the database"
api POST /backup/run '{}' >/dev/null
[ "$(ls "$work/backups"/*.mbk | wc -l)" = 2 ] || fail "the new version did not make a backup after the upgrade"
stop
echo "Upgrade $old_version -> $new_version: nothing lost."
