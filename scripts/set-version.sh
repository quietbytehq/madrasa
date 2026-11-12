#!/usr/bin/env bash
# Set the same version in Cargo.toml, package.json, apps/desktop/package.json and
# apps/desktop/src-tauri/tauri.conf.json, and bring Cargo.lock in step with it.
# Usage: scripts/set-version.sh 0.1.4
#
# Cargo.lock records the version of every crate in this workspace. The Nexus hub image is
# built with `cargo build --locked`, which refuses a lock file that says another version,
# so a version bump that leaves Cargo.lock behind breaks the release. This script stops
# with an error if it cannot update the lock file.
set -euo pipefail
v="${1:?version, e.g. 0.1.4}"
if ! [[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "version must be three numbers, e.g. 0.1.4 (pre-release names go in the tag only)" >&2
  exit 1
fi
cd "$(dirname "$0")/.."
sed -i.bak -E "0,/^version = \"[^\"]+\"/s//version = \"$v\"/" Cargo.toml && rm Cargo.toml.bak
for f in package.json apps/desktop/package.json apps/desktop/src-tauri/tauri.conf.json; do
  sed -i.bak -E "0,/\"version\": \"[^\"]+\"/s//\"version\": \"$v\"/" "$f" && rm "$f.bak"
done
# Rewrite only this workspace's own entries in Cargo.lock; no dependency is upgraded. Works
# without the network when the dependencies are already downloaded, and with it otherwise.
if ! cargo update --workspace --offline >/dev/null 2>&1; then
  cargo update --workspace
fi
scripts/check-lockfile.sh
echo "version $v set in Cargo.toml, Cargo.lock, package.json files and tauri.conf.json"
echo "commit all five files together"
