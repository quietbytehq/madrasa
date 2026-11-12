#!/usr/bin/env bash
# Smoke test for a running Madrasa Nexus hub: health, the web console, first-run setup, login.
#
#   scripts/nexus-smoke.sh http://127.0.0.1:4780 <setup token> [expected version]
#
# Used by CI against the Compose stack. It creates a throwaway administrator, so run it only
# against a hub you are about to discard.
set -euo pipefail

root="${1:?usage: nexus-smoke.sh <base url> <setup token>}"
base="$root/api/nexus/v1"
token="${2:?usage: nexus-smoke.sh <base url> <setup token>}"
password="smoke-test-password-1"

say() { printf '%s\n' "$*"; }
field() { python3 -c "import json,sys; print(json.load(sys.stdin)$1)"; }

health=$(curl -fsS "$base/health")
say "health: $health"
[ "$(printf '%s' "$health" | field "['status']")" = "ok" ]
[ "$(printf '%s' "$health" | field "['database']")" = "ok" ]
# The release name given to the image build must be the version the hub reports.
expected="${3:-}"
if [ -n "$expected" ]; then
  got=$(printf '%s' "$health" | field "['version']")
  [ "$got" = "$expected" ] || { say "expected version $expected, the hub reports $got"; exit 1; }
fi

# The image serves the web console: the page, the script it names, and any console address
# (the browser routes those). The API is not shadowed by it.
page=$(curl -fsS "$root/")
printf '%s' "$page" | grep -q '<title>Madrasa Nexus</title>' || { say "the console page is missing"; exit 1; }
asset=$(printf '%s' "$page" | grep -o '/assets/[^"]*\.js' | head -n 1)
[ -n "$asset" ] || { say "the console page names no script"; exit 1; }
[ "$(curl -s -o /dev/null -w '%{http_code}' "$root$asset")" = "200" ] || { say "the console script did not load"; exit 1; }
curl -fsS "$root/schools" | grep -q '<title>Madrasa Nexus</title>' || { say "a console address did not return the page"; exit 1; }
curl -fsSI "$root/" | grep -qi '^content-security-policy:' || { say "the console is served without a content security policy"; exit 1; }
[ "$(curl -s -o /dev/null -w '%{http_code}' "$base/no-such-thing")" = "404" ] || { say "an unknown API address should be 404"; exit 1; }
say "console: served"

[ "$(curl -fsS "$base/setup/status" | field "['needs_setup']")" = "True" ]

# A wrong token must be refused.
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$base/setup" -H 'content-type: application/json' \
  -d "{\"setup_token\":\"wrong\",\"hub_name\":\"Smoke\",\"admin_username\":\"smoke\",\"admin_full_name\":\"Smoke Test\",\"admin_password\":\"$password\"}")
[ "$code" = "403" ] || { say "expected 403 for a wrong setup token, got $code"; exit 1; }

curl -fsS -X POST "$base/setup" -H 'content-type: application/json' \
  -d "{\"setup_token\":\"$token\",\"hub_name\":\"Smoke Network\",\"admin_username\":\"smoke\",\"admin_full_name\":\"Smoke Test\",\"admin_password\":\"$password\"}" >/dev/null
say "setup: done"

session=$(curl -fsS -X POST "$base/auth/login" -H 'content-type: application/json' \
  -d "{\"username\":\"smoke\",\"password\":\"$password\"}" | field "['token']")
me=$(curl -fsS "$base/auth/me" -H "authorization: Bearer $session")
[ "$(printf '%s' "$me" | field "['hub_name']")" = "Smoke Network" ]
say "login: ok"

# Without a session the API refuses.
code=$(curl -s -o /dev/null -w '%{http_code}' "$base/users")
[ "$code" = "401" ] || { say "expected 401 without a session, got $code"; exit 1; }
say "smoke test passed"
