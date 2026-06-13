#!/bin/bash
set -euo pipefail

# build-android.sh — Cross-compile grafito-ffi for Android
# Requires: NDK 26+, cargo-ndk, rustup targets

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$HOME/Android/Sdk/ndk/26.1.10909125}"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-17-openjdk-amd64}"

echo "=== Installing Android Rust targets ==="
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android 2>/dev/null || true

echo "=== Building grafito-ffi for Android ==="
cd "$PROJECT_ROOT"

OUT_DIR="android/app/src/main/jniLibs"
mkdir -p "$OUT_DIR/arm64-v8a" "$OUT_DIR/armeabi-v7a" "$OUT_DIR/x86_64"

cargo ndk \
    -t arm64-v8a \
    -t armeabi-v7a \
    -t x86_64 \
    -o "$OUT_DIR" \
    build --release -p grafito-ffi

echo "=== Done! Libraries: ==="
find "$OUT_DIR" -name "*.so" -exec ls -lh {} \;

echo "=== Building APK ==="
cd "$PROJECT_ROOT/android"
./gradlew assembleDebug
echo "=== Build finished successfully ==="
