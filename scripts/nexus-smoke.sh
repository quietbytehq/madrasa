#!/usr/bin/env bash
# Smoke test for a running Madrasa Nexus hub: health, first-run setup, login.
#
#   scripts/nexus-smoke.sh http://127.0.0.1:4780 <setup token>
#
# Used by CI against the Compose stack. It creates a throwaway administrator, so run it only
# against a hub you are about to discard.
set -euo pipefail

base="${1:?usage: nexus-smoke.sh <base url> <setup token>}/api/nexus/v1"
token="${2:?usage: nexus-smoke.sh <base url> <setup token>}"
password="smoke-test-password-1"

say() { printf '%s\n' "$*"; }
field() { python3 -c "import json,sys; print(json.load(sys.stdin)$1)"; }

health=$(curl -fsS "$base/health")
say "health: $health"
[ "$(printf '%s' "$health" | field "['status']")" = "ok" ]
[ "$(printf '%s' "$health" | field "['database']")" = "ok" ]

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
