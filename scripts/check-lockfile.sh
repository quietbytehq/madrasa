#!/usr/bin/env bash
# Fail when Cargo.lock does not match the Cargo.toml files: usually a version bump that did
# not include Cargo.lock. The Nexus hub image builds with `--locked` and would fail later
# and less clearly. Run by CI, by the Release workflow and by scripts/set-version.sh.
#
#   scripts/check-lockfile.sh           the version comparison, then cargo's own check
#   scripts/check-lockfile.sh --quick   the version comparison only (needs no cargo)
set -euo pipefail
quick="${1:-}"
cd "$(dirname "$0")/.."
version=$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
locked=$(awk '/^name = "madrasa-nexus"$/ { getline; gsub(/version = |"/, ""); print; exit }' Cargo.lock)
if [ "$version" != "$locked" ]; then
  echo "Cargo.lock is out of date: Cargo.toml says $version, Cargo.lock says $locked." >&2
  echo "Run scripts/set-version.sh $version (or cargo update --workspace) and commit Cargo.lock." >&2
  exit 1
fi
if [ "$quick" = "--quick" ]; then
  echo "Cargo.lock has this workspace at $version, like Cargo.toml"
  exit 0
fi
# The full check: cargo itself must accept the lock file unchanged.
if ! out=$(cargo metadata --locked --format-version 1 2>&1 >/dev/null); then
  echo "Cargo.lock is out of date:" >&2
  echo "$out" >&2
  echo "Run cargo update --workspace (or cargo check) and commit Cargo.lock." >&2
  exit 1
fi
echo "Cargo.lock matches Cargo.toml ($version)"
