#!/usr/bin/env bash
# Rose-Baileys Multiplatform CLI Build Automation (Bash)
# Builds baileys-cli for Linux (glibc & static musl), macOS (Apple Silicon & Intel), and Windows.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${WORKSPACE_ROOT}/dist"
TARGET="${1:-host}"
RELEASE="${RELEASE:-true}"
VERSION="${VERSION:-}"

if [ -z "${VERSION}" ]; then
    VERSION=$(grep -m1 '^version' "${WORKSPACE_ROOT}/Cargo.toml" | sed -E 's/version[[:space:]]*=[[:space:]]*"([^"]+)"/\1/')
fi

echo "============================================================"
echo "   Rose-Baileys CLI Multiplatform Build (Bash, v${VERSION})   "
echo "============================================================"

mkdir -p "${DIST_DIR}"
cd "${WORKSPACE_ROOT}"

PROFILE_FLAG=""
SUBDIR="debug"
if [ "${RELEASE}" = "true" ]; then
    PROFILE_FLAG="--release"
    SUBDIR="release"
fi

build_target() {
    local triple="$1"
    local os="$2"
    echo ""
    echo "[*] Building target: ${triple}..."

    local target_dir="${WORKSPACE_ROOT}/target"

    if [ "${triple}" = "host" ]; then
        cargo build --target-dir "${target_dir}" -p baileys-cli ${PROFILE_FLAG}
        local bin_path="${WORKSPACE_ROOT}/target/${SUBDIR}/baileys-cli"
        if [ "${os}" = "windows" ]; then bin_path="${bin_path}.exe"; fi
    else
        if command -v cross >/dev/null 2>&1; then
            echo "  -> Using 'cross' for ${triple}..."
            cross build --target-dir "${target_dir}" -p baileys-cli --target "${triple}" ${PROFILE_FLAG}
        elif command -v cargo-zigbuild >/dev/null 2>&1; then
            echo "  -> Using 'cargo-zigbuild' for ${triple}..."
            cargo zigbuild --target-dir "${target_dir}" -p baileys-cli --target "${triple}" ${PROFILE_FLAG}
        else
            echo "  -> Using 'cargo build --target ${triple}'..."
            cargo build --target-dir "${target_dir}" -p baileys-cli --target "${triple}" ${PROFILE_FLAG}
        fi

        local bin_ext=""
        if [ "${os}" = "windows" ]; then bin_ext=".exe"; fi
        local bin_path="${WORKSPACE_ROOT}/target/${triple}/${SUBDIR}/baileys-cli${bin_ext}"
    fi

    if [ -f "${bin_path}" ]; then
        echo "  [+] Build succeeded: ${bin_path}"
        local archive_name="baileys-cli-v${VERSION}-${triple}"
        local staging_dir="${DIST_DIR}/staging_${triple}"
        rm -rf "${staging_dir}"
        mkdir -p "${staging_dir}"

        local out_bin_name="baileys-cli"
        if [ "${os}" = "windows" ]; then out_bin_name="baileys-cli.exe"; fi
        cp "${bin_path}" "${staging_dir}/${out_bin_name}"
        if [ -f "${WORKSPACE_ROOT}/README.md" ]; then
            cp "${WORKSPACE_ROOT}/README.md" "${staging_dir}/README.md"
        fi

        if [ "${os}" = "windows" ]; then
            zip -q -r "${DIST_DIR}/${archive_name}.zip" -j "${staging_dir}"/*
            echo "  [+] Packaged archive: ${DIST_DIR}/${archive_name}.zip"
        else
            tar -czf "${DIST_DIR}/${archive_name}.tar.gz" -C "${staging_dir}" .
            echo "  [+] Packaged archive: ${DIST_DIR}/${archive_name}.tar.gz"
        fi
        rm -rf "${staging_dir}"
    else
        echo "  [-] Binary not found: ${bin_path}" >&2
    fi
}

case "${TARGET}" in
    "host")
        OS_TYPE="linux"
        if [[ "$OSTYPE" == "darwin"* ]]; then OS_TYPE="macos"; fi
        if [[ "$OSTYPE" == "msys"* || "$OSTYPE" == "cygwin"* ]]; then OS_TYPE="windows"; fi
        build_target "host" "${OS_TYPE}"
        ;;
    "linux-all")
        build_target "x86_64-unknown-linux-musl" "linux"
        build_target "x86_64-unknown-linux-gnu" "linux"
        build_target "aarch64-unknown-linux-musl" "linux"
        build_target "aarch64-unknown-linux-gnu" "linux"
        ;;
    "macos-all")
        build_target "aarch64-apple-darwin" "macos"
        build_target "x86_64-apple-darwin" "macos"
        ;;
    "windows-all")
        build_target "x86_64-pc-windows-msvc" "windows"
        build_target "aarch64-pc-windows-msvc" "windows"
        ;;
    *)
        OS_TYPE="linux"
        if [[ "${TARGET}" == *"apple"* ]]; then OS_TYPE="macos"; fi
        if [[ "${TARGET}" == *"windows"* ]]; then OS_TYPE="windows"; fi
        build_target "${TARGET}" "${OS_TYPE}"
        ;;
esac

# Checksums
echo ""
echo "[*] Generating SHA256SUMS.txt..."
cd "${DIST_DIR}"
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum *.zip *.tar.gz 2>/dev/null > SHA256SUMS.txt || true
elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 *.zip *.tar.gz 2>/dev/null > SHA256SUMS.txt || true
fi

echo "============================================================"
echo "   Build completed! Artifacts in: ${DIST_DIR}               "
echo "============================================================"
