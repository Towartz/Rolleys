# Paritas dengan WhiskeySockets/Baileys

Dokumen ini melacak apa yang sudah setara dengan Baileys. Status: `done`, `partial`, `missing`.
`done (unit)` = parser teruji dengan node buatan tangan (`notifications.rs`), belum dibuktikan live.
Kolom "Bukti" diisi saat skenario live-test (`live-suite`) atau test integrasi untuk fitur itu lulus.

Baseline (sebelum pekerjaan paritas): `cargo build --workspace` bersih; `cargo test --workspace` lulus
kecuali `baileys-proto extractor::tests::test_parse_waproto_synthetic_js` (gagal sejak sebelumnya,
assertion `message TestChild {` pada parser nested message).

## Event (`BaileysEventMap`, 34 event)

| Event Baileys | Event Rolleys | Status | Bukti |
|---|---|---|---|
| `connection.update` | `Connecting`/`Connected`/`Disconnected`/`QrCode`/`PairingCode` | done | |
| `creds.update` | `CredsUpdate` | partial | |
| `messaging-history.set` | `HistorySync` | done | |
| `messaging-history.status` | - | missing | |
| `chats.upsert` | - | missing | |
| `chats.update` | `ChatUpdate` | done | |
| `lid-mapping.update` | - | missing | |
| `chats.delete` | - | missing | |
| `presence.update` | `Presence` | done | |
| `contacts.upsert` | - | missing | |
| `contacts.update` | `ContactUpdate` | done | |
| `messages.delete` | `MessageRevoked` (sebagian) | partial | |
| `messages.update` | `MessageEdited`, `Receipt` | partial | |
| `messages.media-update` | `MediaUpdated` | done | |
| `messages.upsert` | `Message` | done | |
| `messages.reaction` | - | missing | |
| `message-receipt.update` | `Receipt` (menyatu) | partial | |
| `groups.upsert` | `GroupsUpsert` | done (live) | live-suite `groups` (2026-10-23) |
| `groups.update` | `GroupsUpdate` | done (live) | live-suite `groups` (2026-10-23) |
| `group-participants.update` | `GroupParticipantsUpdate` | done (unit) | |
| `group.join-request` | `GroupJoinRequest` | done (unit) | |
| `group.member-tag.update` | - | missing | |
| `blocklist.set` / `blocklist.update` | `BlocklistUpdate` | partial | |
| `call` | `Call` | done | |
| `labels.edit` | `LabelEdit` | done | |
| `labels.association` | `ChatLabelUpdate`, `MessageLabelUpdate` | done | |
| `newsletter.reaction` | `NewsletterReaction` | done (unit) | |
| `newsletter.view` | `NewsletterView` | done (unit) | |
| `newsletter-participants.update` | `NewsletterParticipantsUpdate` | done (unit) | |
| `newsletter-settings.update` | `NewsletterSettingsUpdate` | done (unit) | |
| `message-capping.update` | - | missing | |
| `chats.lock` | - | missing | |
| `settings.update` | - | missing | |

## Fungsi (`sock.*`)

Rincian per area ada di rencana kerja; ringkasannya:

| Area | Status | Catatan |
|---|---|---|
| Kirim pesan (teks, media, poll, reaksi, lokasi, kontak, tombol, daftar, view-once, album, event, produk) | done | |
| Edit/hapus/forward/pin | done | |
| `readMessages` batch, `markRead` via syncd + `lastMessages` | partial | hanya per pesan |
| Grup (CRUD, peserta, invite, setelan, approval, member-add-mode) | done | event grup belum ada |
| Komunitas | done | 5 perintah CLI belum terdaftar |
| Newsletter/channel | done | event newsletter belum ada |
| Status/story | done | |
| Privasi, blocklist, profil | done | |
| Pairing code, QR, multi-device | done | |
| App-state sync, history sync | done | |
| LID mapping, usync, `onWhatsApp` | done | |
| Bisnis (katalog, produk, order) | partial | detail order, update profil bisnis |
| Panggilan | partial | belum ada `offer` |
| Bot/AI | partial | |
| Auth state (`creds.update`) | partial | belum ada event |

## Binding

| Binding | Status | Catatan |
|---|---|---|
| napi | partial | ~150 method; kurang send tipe baru, status media, newsletter admin, pairing/QR |
| ffi (uniffi) | partial | mirip napi |
| cabi | partial | tanpa callback event; banyak fungsi grup/kirim hilang (verifikasi terhadap `bindings/c/baileys.h`) |

`CredsUpdate` baru dipancarkan dari loop koneksi (pairing, LID, companion finish); jalur `save_creds` lain di `client/` belum.
Binding: napi mengirim event baru sebagai `{event:"raw"}`; ffi melewatinya sampai Fase 5.

Live-suite (`live-suite`, akun uji sendiri): pesan teks/react/edit/revoke/poll/vote terbukti diterima server
(`Receipt` `server_ack` untuk id yang dikembalikan; `error_*` dianggap gagal). Belum ada kontrol negatif yang
memperlihatkan assertion ini benar-benar gagal saat server menolak.
Media roundtrip (live-suite `media`): dokumen 100003 B (bukan kelipatan 16), gambar PNG, audio 5001 B dikirim ke diri sendiri lalu diunduh
kembali dari CDN dan dicocokkan byte-per-byte. Unduhan memakai pesan dari cache/store lokal; belum diuji dari pesan yang diterima akun lain.

Community & newsletter (live-suite `community`, `newsletter`): newsletter 10/10 lulus (create, query, rename, post, react,
view receipt, mute, admin count, subscribed list, delete + verifikasi hilang). Komunitas: create, metadata, subgroup,
list, invite, unlink lulus. `community_leave` oleh pemilik **timeout 15 dtk dan tidak berefek** (node sama dengan Baileys
`communityLeave`; Baileys juga tidak punya fungsi deactivate). Karena itu area komunitas opt-in (`--community`) dan
meninggalkan komunitas stub yang harus dinonaktifkan manual di HP.

Rich message types (live-suite `rich`, self-chat, semua `server_ack`): lokasi, live location, kontak, vCard, daftar kontak, event,
album, view-once (payload teks), forward, JSON, tombol quick-reply, list, reply/quote, comment, pin, unpin.
`server_ack` hanya membuktikan server menerima stanza, bukan bahwa HP merender pesannya (tombol/list sering tidak
dirender untuk akun non-bisnis). Belum dicakup: view-once media sungguhan, album dengan media anak, produk, request-payment.
