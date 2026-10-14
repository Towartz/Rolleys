#!/usr/bin/env bash
# Rose-Baileys Android JNI & UniFFI Build Pipeline (Linux / macOS / CI)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ANDROID_JNI_DIR="$WORKSPACE_ROOT/../AuroraChat_Android/android/app/src/main/jniLibs"
ANDROID_JAVA_DIR="$WORKSPACE_ROOT/../AuroraChat_Android/android/app/src/main/java"

echo "============================================================"
echo "   Rose-Baileys Android UniFFI & JNI Build Automation       "
echo "============================================================"

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
echo "   Rose-Baileys Android UniFFI Setup Complete!              "
echo "============================================================"
