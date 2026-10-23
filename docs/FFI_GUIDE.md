# Rolleys Foreign Function Interface (FFI) Guide

Rolleys provides first-class foreign function interfaces to enable integration into non-Rust ecosystems:
- **C-ABI (`baileys-cabi`)**: Native C headers for C, C++, C#, Flutter / Dart, Python `ctypes`, and Go `cgo`.
- **UniFFI (`baileys-ffi`)**: High-level idiomatic bindings generating Kotlin for Android (JNI) and Swift for Apple platforms (iOS, macOS).
- **Node-API (`baileys-napi`)**: Native addon for Node.js, Bun, and Electron.

---

## 1. C-ABI Integration (`baileys-cabi`)

Headers are located at [`bindings/c/baileys.h`](../bindings/c/baileys.h).

### Building Dynamic / Static Libraries
```bash
# Build release shared library (DLL / SO / DyLib)
cargo build --release -p baileys-cabi
```
Outputs:
- Windows: `target/release/baileys_cabi.dll` & `baileys_cabi.lib`
- Linux / Android: `target/release/libbaileys_cabi.so`
- macOS: `target/release/libbaileys_cabi.dylib`

### Example C Integration
```c
#include "baileys.h"
#include <stdio.h>

void on_event_callback(const char* event_json, void* user_data) {
    printf("[Event Received]: %s\n", event_json);
}

int main() {
    baileys_config_t config = {
        .browser_name = "Chrome",
        .platform = "Mac OS",
        .browser_version = "14.4.1",
        .db_path = "session.db"
    };

    baileys_handle_t* client = baileys_create_client(&config);
    baileys_set_event_callback(client, on_event_callback, NULL);

    int status = baileys_connect(client);
    if (status != 0) {
        printf("Connection failed!\n");
        return 1;
    }

    // Send a message
    baileys_send_text(client, "1234567890@s.whatsapp.net", "Hello from C!");

    // Keep event loop alive
    getchar();

    baileys_destroy_client(client);
    return 0;
}
```

---

## 2. Android Kotlin / JNI (`baileys-ffi`)

Rolleys uses Mozilla's `uniffi-rs` to generate thread-safe JNI bridges without manual JNI C boilerplate.

### Target Architecture Matrix
- `arm64-v8a` (`aarch64-linux-android`)
- `armeabi-v7a` (`armv7-linux-androideabi`)
- `x86_64` (`x86_64-linux-android`)

### Automation Script
```bash
./scripts/build-android-jni.sh
```
Or on Windows PowerShell with Android NDK:
```powershell
.\scripts\build-android-jni.ps1 -TargetAbi all -Release
```

### Kotlin Usage (Android App)
The generated bindings live in package `uniffi.baileys_ffi` (`bindings/kotlin`).
```kotlin
import uniffi.baileys_ffi.*

class WhatsAppService : RolleysEventListener {
    private val client = RolleysClient(
        RolleysClientConfig(
            databasePath = "/data/data/com.example.app/files/rolleys_session.db",
            // Same device identity as Baileys' default (Browsers.macOS("Chrome")).
            browserName = "Chrome",
            platform = "Mac OS",
            phoneNumber = null, // set a number to pair by code instead of QR
            autoDownloadMedia = false,
        )
    )

    fun start() = client.start(this)

    override fun onEvent(event: RolleysEvent) {
        when (event) {
            is RolleysEvent.QrCode -> renderQr(event.code)
            is RolleysEvent.Connected -> onConnected(event.meJid)
            else -> {}
        }
    }

    fun sendMessage(recipientJid: String, text: String): String =
        client.sendText(recipientJid, text)
}
```

---

## 3. Apple Platforms (iOS & macOS Swift)

### Building XCFramework & Swift Package
```bash
./scripts/build-apple.sh --ios
```
This generates a universal fat archive and generates Swift wrappers via `uniffi-bindgen`.

### Swift Usage (iOS / macOS)
Add `bindings/swift/baileys_ffi.swift`, `baileys_ffiFFI.h` and `baileys_ffiFFI.modulemap` to your target.
```swift
final class WhatsAppManager: RolleysEventListener {
    private var client: RolleysClient?

    func initialize() throws {
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let dbPath = docs.appendingPathComponent("rolleys_session.db").path

        // Same device identity as Baileys' default (Browsers.macOS("Chrome")).
        let config = RolleysClientConfig(
            databasePath: dbPath,
            browserName: "Chrome",
            platform: "Mac OS",
            phoneNumber: nil,
            autoDownloadMedia: false
        )
        let client = try RolleysClient(config: config)
        self.client = client
        try client.start(listener: self)
    }

    func onEvent(event: RolleysEvent) {
        switch event {
        case .qrCode(let code): print("QR: \(code)")
        case .connected(let meJid): print("Connected as \(meJid)")
        default: break
        }
    }
}
```

---

## 4. Node.js & Electron (`baileys-napi`)

Native C++ Node-API addon allowing Node.js or Bun services to run Rolleys with near-native performance:

```bash
cd bindings/baileys-napi
npm install
npm run build
```

### JavaScript / TypeScript Usage
```typescript
import { BaileysClient, SqliteStore } from 'rolleys-napi';

const store = SqliteStore.open('./session.db');
const client = new BaileysClient(store, {
    browserName: 'Chrome',
    platform: 'Mac OS',
});

client.on('qr', (qrCode) => {
    console.log('Scan QR:', qrCode);
});

client.on('message', (msg) => {
    console.log('Received message:', msg);
});

await client.connect();
await client.messages.sendText('1234567890@s.whatsapp.net', 'Hello from Node.js!');
```
