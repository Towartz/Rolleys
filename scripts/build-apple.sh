#!/usr/bin/env bash
# Rose-Baileys Apple (macOS & iOS) UniFFI Build Pipeline
# Compiles staticlib / dylib and generates Swift bindings / XCFramework for iOS & macOS.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$WORKSPACE_ROOT/bindings/swift"

echo "============================================================"
echo "   Rose-Baileys Apple (macOS & iOS) Build Automation        "
echo "============================================================"

cd "$WORKSPACE_ROOT"

# 1. Build host uniffi-bindgen tool
echo "[1/3] Building uniffi-bindgen tool..."
cargo build -p baileys-ffi --bin uniffi-bindgen

# 2. Compile host dylib and generate Swift bindings
echo "[2/3] Generating Swift bindings..."
cargo build -p baileys-ffi
HOST_LIB="$WORKSPACE_ROOT/target/debug/libbaileys_ffi.dylib"
if [ ! -f "$HOST_LIB" ]; then
    HOST_LIB="$WORKSPACE_ROOT/target/debug/libbaileys_ffi.so"
fi
if [ ! -f "$HOST_LIB" ]; then
    HOST_LIB="$WORKSPACE_ROOT/target/debug/baileys_ffi.dll"
fi

mkdir -p "$OUT_DIR"
cargo run -p baileys-ffi --bin uniffi-bindgen -- generate \
    --library "$HOST_LIB" \
    --language swift \
    --no-format \
    --out-dir "$OUT_DIR"

echo "  [+] Swift bindings generated in: $OUT_DIR"

# 3. Optional: Compile iOS targets if requested
if [ "${1:-}" = "--ios" ]; then
    echo "[3/3] Compiling iOS static targets (aarch64-apple-ios, aarch64-apple-ios-sim)..."
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim 2>/dev/null || true
    cargo build -p baileys-ffi --release --target aarch64-apple-ios
    cargo build -p baileys-ffi --release --target aarch64-apple-ios-sim
    echo "  [+] iOS static libraries built successfully."
fi

echo "============================================================"
echo "   Apple Build Setup Completed!                             "
echo "============================================================"
