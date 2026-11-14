#!/usr/bin/env bash
# Build the parent app's Android package (APK), unsigned. CI runs this on every pull
# request and the Release workflow runs it for a release, so both build it the same way.
# Sign the result with scripts/sign-apk.sh.
#
# Needs: the Android SDK (ANDROID_HOME) with an NDK, Java 17, Rust with the targets
# aarch64-linux-android and armv7-linux-androideabi, and `pnpm install` done.
#
#   scripts/build-parent-apk.sh            writes dist-apk/madrasa-parent-unsigned.apk
#
# The Android project is generated (`tauri android init`) into apps/parent/src-tauri/gen,
# which is not kept in the repository.
set -euo pipefail
cd "$(dirname "$0")/.."

: "${ANDROID_HOME:?ANDROID_HOME is not set: it must point at the Android SDK}"
# Tauri looks for the NDK in NDK_HOME. GitHub's runners name theirs differently.
export NDK_HOME="${NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}}}"
if [ -z "$NDK_HOME" ] || [ ! -d "$NDK_HOME" ]; then
  echo "No Android NDK found: set NDK_HOME to its folder (Android Studio: SDK Manager, SDK Tools, NDK)." >&2
  exit 1
fi

app=apps/parent
android="$app/src-tauri/gen/android"
if [ ! -d "$android" ]; then
  echo "Generating the Android project"
  pnpm --dir "$app" tauri android init --ci
fi

# `init` gives the app Tauri's own launcher icon; replace it with Madrasa's, generated
# from assets/brand/madrasa-icon.svg by assets/brand/generate-icons.sh.
cp -r "$app/src-tauri/icons/android/." "$android/app/src/main/res/"

# Phones: 64-bit and 32-bit ARM, in one package.
pnpm --dir "$app" tauri android build --apk --target aarch64 --target armv7

built=$(find "$android/app/build/outputs/apk" -name '*.apk' | sort | head -n 1)
if [ -z "$built" ]; then
  echo "The build finished but no APK was found under $android/app/build/outputs/apk." >&2
  exit 1
fi
mkdir -p dist-apk
cp "$built" dist-apk/madrasa-parent-unsigned.apk
echo "Built $built"
ls -l dist-apk/madrasa-parent-unsigned.apk
