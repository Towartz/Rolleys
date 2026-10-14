# Rolleys API Reference Guide

This document is an exhaustive API reference for **`baileys-core`** and associated workspace crates.

---

## Table of Contents
1. [Core Engine: BaileysClient](#core-engine-baileysclient)
2. [Configuration: BaileysConfig & Builder](#configuration-baileysconfig--builder)
3. [Reactive Event Stream: BaileysEvent](#reactive-event-stream-baileysevent)
4. [Domain Service: MessageService](#domain-service-messageservice)
5. [Domain Service: GroupService](#domain-service-groupservice)
6. [Domain Service: NewsletterService](#domain-service-newsletterservice)
7. [Domain Service: StatusService](#domain-service-statusservice)
8. [Domain Service: PrivacyService](#domain-service-privacyservice)
9. [Persistence Backends: BaileysStore](#persistence-backends-baileysstore)
10. [Error Types: CoreError & StoreError](#error-types-coreerror--storeerror)

---

## Core Engine: BaileysClient

The primary entry point for managing WhatsApp connections, state transitions, and service access.

```rust
use baileys_core::{BaileysClient, BaileysConfig, BaileysEvent};
use baileys_store::BaileysStore;
use std::sync::Arc;
```

### Methods

#### `new(store: Arc<dyn BaileysStore>, config: BaileysConfig) -> BaileysClient`
Constructs a new client instance backed by the supplied storage engine and runtime configuration.

#### `async connect(&self) -> Result<(), CoreError>`
Initializes the TLS transport, performs the Noise handshake, loads or generates session authentication credentials, and launches the background stanza processing loop.

#### `events(&self) -> broadcast::Receiver<BaileysEvent>`
Subscribes to the broadcast channel of protocol events (QR updates, inbound messages, connection status, receipts).

#### `is_connected(&self) -> bool`
Returns atomic boolean status indicating whether the client is authenticated and currently connected to the WhatsApp gateway.

#### `get_connection_state(&self) -> ConnectionState`
Returns the formal finite state machine enum: `Disconnected`, `Connecting`, `Connected`, `Reconnecting`, `LoggedOut`.

#### `subscribe_connection_state(&self) -> watch::Receiver<ConnectionState>`
Returns a reactive watch channel that fires on formal FSM state transitions.

#### `telemetry_snapshot(&self) -> TelemetrySnapshot`
Captures real-time metrics: messages sent, messages received, retry counts, bytes transmitted, active ratchets.

#### `is_ghost_mode(&self) -> bool` / `set_ghost_mode(&self, enabled: bool)`
Toggles Ghost Mode: automatically suppresses read receipts (`read`) and status story view receipts towards remote contacts.

#### Domain Service Accessors
- `client.messages() -> MessageService`
- `client.groups() -> GroupService`
- `client.newsletters() -> NewsletterService`
- `client.status() -> StatusService`
- `client.privacy() -> PrivacyService`

---

## Configuration: BaileysConfig & Builder

```rust
use baileys_core::BaileysConfigBuilder;

let config = BaileysConfigBuilder::new()
    .with_browser("Rolleys", "Linux", "1.0.0")
    .with_phone_pairing("+1234567890") // Optional phone pairing mode
    .with_anti_revoke(true)            // Intercept message deletions
    .with_auto_save_view_once(true)    // Save view-once media to disk
    .with_ghost_mode(false)            // Suppress read receipts
    .build();
```

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `browser_name` | `String` | `"Chrome"` | Client browser string sent during registration |
| `platform` | `String` | `"Mac OS"` | Host OS identifier (or auto-detected via `.with_current_platform()`) |
| `browser_version` | `String` | `"14.4.1"` | Browser version string |
| `wa_websocket_url` | `String` | `"wss://web.whatsapp.com/ws/chat"` | Gateway WebSocket endpoint |
| `connect_timeout_ms` | `u64` | `20_000` | Gateway connection timeout in milliseconds |
| `keep_alive_interval_ms` | `u64` | `20_000` | Keep-alive ping interval |
| `phone_number_pairing` | `Option<String>` | `None` | E.164 phone number for 8-digit code pairing |
| `anti_revoke` | `bool` | `true` | Retains deleted message contents in local store |
| `auto_save_view_once` | `bool` | `true` | Automatically decrypts and persists ephemeral view-once media |
| `ghost_mode` | `bool` | `false` | Suppresses outgoing read receipts and status views |

---

## Reactive Event Stream: BaileysEvent

```rust
pub enum BaileysEvent {
    Connecting,
    Connected { me_jid: String },
    Disconnected { reason: String },
    QrCode { code: String },
    PairingCode { code: String },
    Message { message: Box<WebMessageInfo> },
    Receipt { key_id: String, remote_jid: String, status: String },
    Presence { jid: String, presence: String },
    HistorySync { sync_type: i32, progress: u32, conversations: Vec<Conversation>, status_v3_messages: Vec<WebMessageInfo> },
    PollVote { poll_id: String, voter_jid: String, selected_options: Vec<String>, tally: PollTally },
    Call { call: CallEvent },
    StoryReceived { story: StoredStory },
    BlocklistUpdate { added: Vec<String>, removed: Vec<String> },
    MessageEdited { chat_jid: String, message_id: String, new_text: String },
    MessageRevoked { chat_jid: String, message_id: String },
}
```

---

## Domain Service: MessageService

Access via `client.messages()`.

### Text & Quotes
```rust
// Standard text
let msg_id = client.messages().send_text("62812345678@s.whatsapp.net", "Hello").await?;

// Quoted reply
let msg_id = client.messages().send_text_with_quote(
    "62812345678@s.whatsapp.net",
    "Reply text",
    "QUOTED_MESSAGE_ID",
    "Quoted preview snippet",
    None, // Participant JID if quoting in a group
).await?;
```

### Media (Images, Videos, Audio, Documents)
```rust
// Encrypted image with thumbnail & caption
let image_bytes = std::fs::read("photo.jpg")?;
let msg_id = client.messages().send_image(
    "62812345678@s.whatsapp.net",
    &image_bytes,
    Some("Image caption"),
).await?;

// Document with filename
let doc_bytes = std::fs::read("document.pdf")?;
let msg_id = client.messages().send_document(
    "62812345678@s.whatsapp.net",
    &doc_bytes,
    "invoice.pdf",
    "application/pdf",
    Some("Here is your invoice"),
).await?;
```

### Reactions & Ephemeral Controls
```rust
// React with emoji
client.messages().send_reaction("62812345678@s.whatsapp.net", "TARGET_MSG_ID", "❤️").await?;

// Remove reaction
client.messages().send_reaction("62812345678@s.whatsapp.net", "TARGET_MSG_ID", "").await?;

// Revoke / Delete for everyone
client.messages().revoke_message("62812345678@s.whatsapp.net", "TARGET_MSG_ID").await?;
```

### Interactive Polls
```rust
let poll_id = client.messages().send_poll(
    "12036302@g.us",
    "Preferred meeting time?",
    vec!["10:00 AM".into(), "02:00 PM".into(), "04:30 PM".into()],
    false, // allow_multiple_answers
).await?;
```

---

## Domain Service: GroupService

Access via `client.groups()`.

```rust
// Create a new group
let group_jid = client.groups().create_group(
    "Engineering Team",
    vec!["6281111111@s.whatsapp.net".into(), "6282222222@s.whatsapp.net".into()],
).await?;

// Fetch group metadata
let metadata = client.groups().get_group_metadata(&group_jid).await?;
println!("Group subject: {}, participants: {}", metadata.subject, metadata.participants.len());

// Add participant
client.groups().add_participants(&group_jid, vec!["6283333333@s.whatsapp.net".into()]).await?;

// Promote to admin
client.groups().promote_participants(&group_jid, vec!["6283333333@s.whatsapp.net".into()]).await?;

// Generate invite link code
let invite_code = client.groups().get_invite_code(&group_jid).await?;
println!("https://chat.whatsapp.com/{invite_code}");
```

---

## Domain Service: NewsletterService

Access via `client.newsletters()`.

```rust
// Create a WhatsApp Channel
let channel = client.newsletters().create(
    "Rolleys Announcements",
    Some("Official channel for Rolleys releases"),
    None, // picture_bytes
).await?;

// Post an announcement
client.newsletters().send_text(&channel.jid, "Release v0.1.0 is now live!").await?;

// Follow / Subscribe to a channel
client.newsletters().follow(&channel.jid).await?;
```

---

## Persistence Backends: BaileysStore

### Embedded SQLite (`SqliteStore`)
```rust
use baileys_store::SqliteStore;
let store = Arc::new(SqliteStore::open("session.db")?);
```
- Full transactional persistence for credentials, cryptographic ratchets, chats, messages, and contact sync.
- Configured with `WAL` journaling mode and `NORMAL` synchronous pragma.

### Multi-File JSON Store (`FileStore`)
```rust
use baileys_store::FileStore;
let store = Arc::new(FileStore::new("session_directory"));
```
- 100% file-compatible with Baileys Node.js `useMultiFileAuthState`.

### In-Memory Store (`MemoryStore`)
```rust
use baileys_store::MemoryStore;
let store = Arc::new(MemoryStore::new());
```
- Volatile storage ideal for stateless test suites, serverless lambdas, and ephemeral bots.
