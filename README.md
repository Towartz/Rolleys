# Rolleys: High-Performance Multiplatform WhatsApp Engine

A pure Rust WhatsApp Web API implementation designed for high-performance multiplatform deployment across Desktop, Mobile, Embedded, and Web runtimes.

---

## 🚀 Target Platforms & Architecture

| Platform | Target Architecture | Distribution / Interface | Crate / Artifact |
| :--- | :--- | :--- | :--- |
| **Windows** | `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` | Tauri Desktop / CLI / DLL / StaticLib | `estella-desktop`, `baileys-cli`, `baileys_cabi.dll` |
| **macOS** | `aarch64-apple-darwin`, `x86_64-apple-darwin` | Swift App / Tauri / DyLib | `baileys-ffi` (Swift), `baileys_cabi.dylib` |
| **Linux** | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | CLI / Daemon / Docker / Shared Object | `baileys-cli`, `libbaileys_cabi.so` |
| **Android** | `arm64-v8a`, `armeabi-v7a`, `x86_64` | Kotlin / JNI via UniFFI | `AuroraChat_Android`, `libbaileys_ffi.so` |
| **iOS** | `aarch64-apple-ios`, `aarch64-apple-ios-sim` | Swift Package / XCFramework | `bindings/swift`, `libbaileys_ffi.a` |
| **Node.js** | Node.js 18+ / Electron / Bun | N-API Native Addon | `baileys-napi` |

---

## 📦 Workspace Crates

- **`baileys-core`**: Core WhatsApp client protocol, session management, stanzas, QR code, phone-number pairing, and event loop.
- **`baileys-crypto`**: Cryptographic primitives (Curve25519, Ed25519, AES-GCM, AES-CBC, AES-CTR, HKDF, HMAC-SHA256, LtHash) implemented in 100% portable pure Rust.
- **`baileys-proto`**: WhatsApp Protobuf definitions and codecs compiled via portable build script with vendored protoc fallback.
- **`baileys-media`**: CDN media encryption, decryption, streaming upload/download, and cross-platform filename sanitization.
- **`baileys-noise`**: WhatsApp Noise Protocol framework (`Noise_XX_25519_AESGCM_SHA256`).
- **`baileys-signal`**: Signal Double Ratchet protocol, Session Cipher, and Group Cipher implementations.
- **`baileys-store`**: SQLite and in-memory persistent storage with automatic directory provisioning for mobile/desktop.
- **`baileys-ffi`**: Multiplatform UniFFI bindings generating Kotlin (Android) and Swift (iOS/macOS).
- **`baileys-cabi`**: Universal C-ABI (`bindings/c/baileys.h`) for integration with C, C++, C#, Flutter, Dart, Python, and Go.
- **`baileys-napi`**: Node-API binding for seamless integration with Node.js and Electron apps.
- **`baileys-cli`**: Command-line interface for session pairing, chat inspection, and testing.
- **`baileys-live-test`**: Isolated live WhatsApp gateway integration test suite and status broadcast test harness.

---

## 🛠️ Multiplatform Features & Enhancements

1. **Embedded WebPKI TLS Roots**: Configured `webpki-roots` alongside `native-roots` for `tokio-tungstenite`, ensuring out-of-the-box secure WebSocket connections on Android, iOS, and minimal Docker containers where native system root certificates are unavailable.
2. **Cross-Platform Crate Types**: `baileys-ffi` and `baileys-cabi` produce both `cdylib` (for Android `.so` / Windows `.dll` / macOS `.dylib`) and `staticlib` (for iOS `.a` static archives and embedded linkers).
3. **Resilient Protobuf Build**: `baileys-proto/build.rs` dynamically checks for system-provided `PROTOC` before falling back to `protoc-bin-vendored`, supporting Termux, Nix, Yocto, and cross-compilation environments.
4. **Universal Path Sanitization**: `sanitize_filename` in `baileys-media` handles both Windows (`\`) and Unix (`/`) delimiters and protects against Windows MS-DOS device collisions (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`).
5. **Auto-Directory Creation**: `SqliteStore::open` automatically ensures parent directory existence before opening database files on Android sandbox paths (`/data/data/...`) and Unix systems.
6. **Platform Identification**: `BaileysConfig::builder().with_current_platform()` dynamically resolves the host OS string ("Windows", "Mac OS", "Linux", "Android", "iOS") during WhatsApp handshake registration.

---

## 🔧 Building for Platforms

### Host Desktop (Windows, macOS, Linux)
```bash
cargo build --workspace
cargo test --workspace --lib
```

### Android (UniFFI & Kotlin)
```powershell
# PowerShell
.\scripts\build-android-jni.ps1 -TargetAbi all -Release
```
```bash
# Bash / Linux / macOS
./scripts/build-android-jni.sh
```

### Apple (iOS & macOS Swift)
```bash
# Generate Swift bindings and optionally build iOS static targets
./scripts/build-apple.sh --ios
```

### C-ABI (C / C++ / Embedded)
Headers are located at `bindings/c/baileys.h`. Link against `target/release/baileys_cabi.dll` (Windows), `libbaileys_cabi.so` (Linux/Android), or `libbaileys_cabi.a` (static).
