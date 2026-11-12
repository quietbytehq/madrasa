#!/usr/bin/env bash
# Sign an Android package so that it can be installed.
#
#   scripts/sign-apk.sh <unsigned.apk> <output-dir> <name-without-extension>
#
# With the release key (all four variables set), the output is <name>.apk:
#   ANDROID_KEYSTORE_BASE64     the keystore file, base64-encoded
#   ANDROID_KEYSTORE_PASSWORD   its password
#   ANDROID_KEY_ALIAS           the key's alias in the keystore
#   ANDROID_KEY_PASSWORD        the key's password (the keystore password when left out)
#
# Without ANDROID_KEYSTORE_BASE64 the package is signed with a key made for this one build
# and thrown away, and the output is <name>-testkey.apk. Such a package installs and runs,
# for testing. It cannot update an earlier install (Android refuses a different key: the
# old app must be removed first, which also removes its sign-in), and it must not be
# handed to parents as the real app.
#
# Either way <output-dir>/SIGNING.txt says which key was used. No password, key or keystore
# is written to the log or left on disk.
set -euo pipefail
unsigned="${1:?the unsigned APK}"
out="${2:?the output folder}"
name="${3:?the output name, without .apk}"

: "${ANDROID_HOME:?ANDROID_HOME is not set: it must point at the Android SDK}"
tools=$(ls -d "$ANDROID_HOME"/build-tools/* 2>/dev/null | sort -V | tail -n 1)
if [ -z "$tools" ] || [ ! -x "$tools/apksigner" ]; then
  echo "No Android build-tools found under $ANDROID_HOME/build-tools (apksigner, zipalign)." >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

if [ -n "${ANDROID_KEYSTORE_BASE64:-}" ]; then
  : "${ANDROID_KEYSTORE_PASSWORD:?ANDROID_KEYSTORE_PASSWORD is not set}"
  : "${ANDROID_KEY_ALIAS:?ANDROID_KEY_ALIAS is not set}"
  export ANDROID_KEY_PASSWORD="${ANDROID_KEY_PASSWORD:-$ANDROID_KEYSTORE_PASSWORD}"
  if ! printf '%s' "$ANDROID_KEYSTORE_BASE64" | base64 --decode > "$work/keystore" 2>/dev/null; then
    echo "ANDROID_KEYSTORE_BASE64 is not valid base64. Encode the keystore file with: base64 -w0 <file>" >&2
    exit 1
  fi
  final="$out/$name.apk"
  kind="the release key"
else
  # A key for this build only. The password never leaves this process.
  ANDROID_KEYSTORE_PASSWORD=$(head -c 32 /dev/urandom | base64 | tr -dc 'A-Za-z0-9' | head -c 32)
  export ANDROID_KEYSTORE_PASSWORD
  export ANDROID_KEY_PASSWORD="$ANDROID_KEYSTORE_PASSWORD"
  ANDROID_KEY_ALIAS="testkey"
  keytool -genkeypair -keystore "$work/keystore" -storetype PKCS12 \
    -storepass:env ANDROID_KEYSTORE_PASSWORD -keypass:env ANDROID_KEY_PASSWORD \
    -alias "$ANDROID_KEY_ALIAS" -keyalg RSA -keysize 2048 -validity 30 \
    -dname "CN=Madrasa Parent test build, O=Not for publishing" >/dev/null 2>&1
  final="$out/$name-testkey.apk"
  kind="a throwaway test key"
fi

"$tools/zipalign" -f -p 4 "$unsigned" "$work/aligned.apk"
"$tools/apksigner" sign \
  --ks "$work/keystore" --ks-key-alias "$ANDROID_KEY_ALIAS" \
  --ks-pass env:ANDROID_KEYSTORE_PASSWORD --key-pass env:ANDROID_KEY_PASSWORD \
  --out "$final" "$work/aligned.apk"
rm -f "$final.idsig"

# The certificate's fingerprint is public; it tells one key from another.
fingerprint=$("$tools/apksigner" verify --print-certs "$final" | grep -m1 'SHA-256' | sed 's/.*: //')
if [ -z "$fingerprint" ]; then
  echo "The signed package did not verify." >&2
  exit 1
fi

{
  echo "File: $(basename "$final")"
  echo "Signed with: $kind"
  echo "Certificate SHA-256: $fingerprint"
  if [ "$kind" != "the release key" ]; then
    echo
    echo "This is a test build. It installs and runs, but it cannot update an earlier"
    echo "install of the app (remove the old one first), and it is not the real app for"
    echo "parents. A release signed with the release key has no \"-testkey\" in its name."
  fi
} > "$out/SIGNING.txt"

echo "Signed $(basename "$final") with $kind (certificate SHA-256 $fingerprint)"
if [ "$kind" != "the release key" ]; then
  echo "::notice title=Parent app APK::No signing key is configured, so this build is signed with a throwaway test key. See docs/RELEASING.md."
fi
