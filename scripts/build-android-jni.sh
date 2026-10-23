#!/usr/bin/env bash
# Rolleys Android JNI & UniFFI Build Pipeline (Linux / macOS / CI)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# Android app project that receives the .so files and Kotlin bindings. Order:
#   1. $ANDROID_PROJECT_DIR when set;
#   2. the parent folder when this repo is its submodule (Rolleys_android/Rolleys);
#   3. a sibling checkout, ../Rolleys_android.
is_gradle_project() {
    [ -f "$1/settings.gradle.kts" ] || [ -f "$1/settings.gradle" ]
}
if [ -z "${ANDROID_PROJECT_DIR:-}" ]; then
    if is_gradle_project "$WORKSPACE_ROOT/.."; then
        ANDROID_PROJECT_DIR="$WORKSPACE_ROOT/.."
    else
        ANDROID_PROJECT_DIR="$WORKSPACE_ROOT/../Rolleys_android"
    fi
fi
if ! is_gradle_project "$ANDROID_PROJECT_DIR"; then
    echo "ERROR: no Android project at $ANDROID_PROJECT_DIR (expected settings.gradle.kts)." >&2
    echo "       Set ANDROID_PROJECT_DIR to the Android app checkout." >&2
    exit 1
fi
ANDROID_PROJECT_DIR="$(cd "$ANDROID_PROJECT_DIR" && pwd)"
# The app module sits at the project root (app/); older layouts nested it under android/.
ANDROID_APP_DIR="$ANDROID_PROJECT_DIR/app"
if [ ! -d "$ANDROID_APP_DIR" ] && [ -d "$ANDROID_PROJECT_DIR/android/app" ]; then
    ANDROID_APP_DIR="$ANDROID_PROJECT_DIR/android/app"
fi
ANDROID_JNI_DIR="$ANDROID_APP_DIR/src/main/jniLibs"
ANDROID_JAVA_DIR="$ANDROID_APP_DIR/src/main/java"

echo "============================================================"
echo "   Rolleys Android UniFFI & JNI Build Automation       "
echo "============================================================"
echo "  Android project: $ANDROID_PROJECT_DIR"

# 1. Build host uniffi-bindgen tool
echo "[1/3] Building host uniffi-bindgen tool..."
cd "$WORKSPACE_ROOT"
cargo build -p baileys-ffi --bin uniffi-bindgen

# 2. Generate UniFFI Kotlin bindings
echo "[2/3] Generating UniFFI Kotlin bindings..."
cargo build -p baileys-ffi
HOST_LIB="$WORKSPACE_ROOT/target/debug/libbaileys_ffi.so"
if [ ! -f "$HOST_LIB" ]; then
    HOST_LIB="$WORKSPACE_ROOT/target/debug/libbaileys_ffi.dylib"
fi
if [ ! -f "$HOST_LIB" ]; then
    HOST_LIB="$WORKSPACE_ROOT/target/debug/baileys_ffi.dll"
fi

cargo run -p baileys-ffi --bin uniffi-bindgen -- generate \
    --library "$HOST_LIB" \
    --language kotlin \
    --no-format \
    --out-dir "$ANDROID_JAVA_DIR"

echo "  [+] Kotlin bindings updated in: $ANDROID_JAVA_DIR/uniffi/baileys_ffi/"

# 3. Cross-compile Android ABIs
echo "[3/3] Cross-compiling Android ABIs..."
if command -v cargo-ndk &> /dev/null; then
    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o "$ANDROID_JNI_DIR" build --release -p baileys-ffi
    echo "  [+] Android JNI libraries installed to: $ANDROID_JNI_DIR"
else
    echo "  [!] cargo-ndk not found. Install via: cargo install cargo-ndk"
fi

echo "============================================================"
echo "   Rolleys Android UniFFI Setup Complete!              "
echo "============================================================"
