#!/usr/bin/env bash
# Set the same version in Cargo.toml, package.json, apps/desktop/package.json and
# apps/desktop/src-tauri/tauri.conf.json, then refresh Cargo.lock.
# Usage: scripts/set-version.sh 0.1.4
set -euo pipefail
v="${1:?version, e.g. 0.1.4}"
cd "$(dirname "$0")/.."
sed -i.bak -E "0,/^version = \"[^\"]+\"/s//version = \"$v\"/" Cargo.toml && rm Cargo.toml.bak
for f in package.json apps/desktop/package.json apps/desktop/src-tauri/tauri.conf.json; do
  sed -i.bak -E "0,/\"version\": \"[^\"]+\"/s//\"version\": \"$v\"/" "$f" && rm "$f.bak"
done
cargo metadata --format-version 1 --offline >/dev/null 2>&1 || true
echo "version $v set in Cargo.toml, package.json files and tauri.conf.json; commit Cargo.lock too"
