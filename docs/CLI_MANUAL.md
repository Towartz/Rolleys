# Rolleys CLI Manual

`baileys-cli` provides an interactive terminal REPL and diagnostic automation tool for the Rolleys WhatsApp engine.

---

## 1. Installation & Execution

### Running via Cargo
```bash
# Standard interactive launch
cargo run -p baileys-cli

# Connect using phone number pairing instead of QR code
cargo run -p baileys-cli -- --phone 628123456789

# Specify custom SQLite session database path
cargo run -p baileys-cli -- --session ./my_session.db

# Invert QR colors (for dark-on-light terminal backgrounds)
cargo run -p baileys-cli -- --invert

# Enable verbose debug logging
cargo run -p baileys-cli -- --debug

# Execute non-interactive automated self-test
cargo run -p baileys-cli -- --self-test
```

---

## 2. Pairing Workflows

### QR Code Pairing (Default)
1. Run `cargo run -p baileys-cli`.
2. When the engine connects, request the QR display:
   ```text
   rolleys> qr
   ```
   Or for raw terminal scaling:
   ```text
   rolleys> qr large
   ```
3. Open WhatsApp on your phone: **Settings > Linked Devices > Link a Device**.
4. Scan the rendered QR code. Upon authentication, the REPL will print:
   ```text
   [AUTH] Authenticated successfully as: 62812345678@s.whatsapp.net
   ```

### Phone Number Code Pairing
1. Run with `--phone <E.164>`:
   ```bash
   cargo run -p baileys-cli -- --phone 628123456789
   ```
2. The terminal outputs an 8-character code:
   ```text
   [PAIR] Pairing Code: ABCD-1234
   ```
3. On your phone, tap **Link with phone number instead** and enter the code.

---

## 3. Interactive REPL Command Reference

| Command | Usage | Description |
| :--- | :--- | :--- |
| `help` | `help [command]` | Lists all available commands or inspects detailed syntax for a specific command |
| `qr` | `qr [large\|raw]` | Displays the latest active pairing QR code with clean half-block unicode rendering |
| `chats` | `chats [limit]` | Lists stored conversations sorted by recent activity |
| `history` | `history <jid> [limit]` | Prints recent message exchange with a specific contact or group |
| `send` | `send <jid> <text>` | Sends a plaintext message to the target JID |
| `reply` | `reply <jid> <msg_id> <text>` | Sends a quoted reply referencing an earlier message |
| `sendimage` | `sendimage <jid> <path> [caption]` | Encrypts, uploads, and dispatches a JPEG/PNG image |
| `sendvideo` | `sendvideo <jid> <path> [caption]` | Streams, encrypts, and transmits an MP4 video |
| `senddoc` | `senddoc <jid> <path> [caption]` | Sends an arbitrary document or PDF attachment |
| `react` | `react <jid> <msg_id> <emoji>` | Dispatches an emoji reaction to a target message |
| `poll` | `poll <jid> <question> <opt1,opt2...>` | Publishes an interactive multi-choice poll |
| `ghost` | `ghost [on\|off]` | Toggles Ghost Mode (suppresses outgoing read receipts and status views) |
| `status` | `status` | Prints live connection state, identity JID, and telemetry counters |
| `bot` | `bot [off\|self\|public]` | Controls the embedded In-Chat Command Runner bot mode |
| `purge` | `purge` | Closes connection and deletes local SQLite session database |
| `clear` | `clear` | Clears terminal screen buffer |
| `exit` / `quit` | `exit` | Gracefully shuts down connection loop and exits |

---

## 4. In-Chat Command Runner (`bot` mode)

Rolleys CLI features a built-in automated bot engine that can process incoming messages in chats:

- `bot off`: Disabled (default).
- `bot self`: Only executes commands when received from your own phone (Note to Self).
- `bot public`: Responds to authorized commands from group chats and contacts.

### Available In-Chat Bot Triggers
- `!ping`: Calculates round-trip gateway latency and echoes system health.
- `!status`: Reports engine connection state, phone JID, LID, and storage statistics.
- `!menu`: Renders an interactive command directory directly in the WhatsApp chat.
