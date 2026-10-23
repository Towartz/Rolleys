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
| `messaging-history.set` | `HistorySync` | done (live: on-demand) | `history --chat-group DXT`: 10 pesan, `is_latest=None`, session id |
| `messaging-history.status` | `MessagingHistoryStatus` | done (unit) | hanya terjadi saat pairing |
| `chats.upsert` | `ChatsUpsert` | done (live) | `groups`: dipancarkan saat grup dibuat (satu-satunya pemicu di Baileys) |
| `chats.update` | `ChatUpdate` | done | |
| `lid-mapping.update` | `LidMappingUpdate` (dari syncd) | partial (unit) | |
| `chats.delete` | `ChatsDelete` | done (unit; live: tidak dipancarkan saat sinkronisasi awal) | |
| `presence.update` | `Presence` | done | |
| `contacts.upsert` | `ContactsUpsert` | partial | hanya dari `lidContactAction`; belum dari history sync |
| `contacts.update` | `ContactUpdate` | done | |
| `messages.delete` | `MessagesDelete`, `MessageRevoked` | partial | deleteMessageForMe (unit); revoke tetap `MessageRevoked` |
| `messages.update` | `MessageEdited`, `Receipt`, `MessageStarred` | partial | starred: live (`appstate`) |
| `messages.media-update` | `MediaUpdated` | done | |
| `messages.upsert` | `Message` | done | |
| `messages.reaction` | `MessagesReaction` | done (live) | `reaction` (interaktif): reaksi dari HP via deviceSentMessage |
| `message-receipt.update` | `Receipt` (menyatu) | partial | |
| `groups.upsert` | `GroupsUpsert` | done (live) | live-suite `groups` (2026-10-23) |
| `groups.update` | `GroupsUpdate` | done (live) | live-suite `groups` (2026-10-23) |
| `group-participants.update` | `GroupParticipantsUpdate` | done (unit) | |
| `group.join-request` | `GroupJoinRequest` | done (unit) | |
| `group.member-tag.update` | `GroupMemberTagUpdate` | done (unit) | `participant_alt` belum diisi |
| `blocklist.set` / `blocklist.update` | `BlocklistUpdate` | partial | |
| `call` | `Call` | done | |
| `labels.edit` | `LabelEdit` | done | |
| `labels.association` | `ChatLabelUpdate`, `MessageLabelUpdate` | done | |
| `newsletter.reaction` | `NewsletterReaction` | done (unit) | |
| `newsletter.view` | `NewsletterView` | done (unit) | |
| `newsletter-participants.update` | `NewsletterParticipantsUpdate` | done (unit) | |
| `newsletter-settings.update` | `NewsletterSettingsUpdate` | done (unit) | |
| `message-capping.update` | `MessageCappingUpdate` | done (unit) | notifikasi `mex` dari server, tak bisa dipicu |
| `chats.lock` | `ChatsLock` | done (live) | `appstate`: 23 event dari snapshot |
| `settings.update` | `SettingsUpdate` | done (live) | `appstate`: 9 event |

## Fungsi (`sock.*`)

Dijaga otomatis: test kontrak memastikan setiap method publik `BaileysClient` terjangkau dari ffi, napi, dan C
(atau dikecualikan dengan alasan), dan setiap kunci `makeWASocket` Baileys 7.0.0-rc14 ada di socket napi atau
dikecualikan dengan alasan. Daftar pengecualian ada di test masing-masing.

| Area | Status | Bukti / catatan |
|---|---|---|
| Kirim pesan (teks, media, poll, reaksi, lokasi, kontak, tombol, daftar, view-once, album, event, produk) | done (live) | area `messaging`, `rich`, `media` |
| Edit/hapus/forward/pin | done (live) | |
| `readMessages` batch | done (unit) | dikelompokkan per chat, `read-self` bila receipt baca dimatikan; belum diuji live agar tidak menandai pesan orang lain sebagai dibaca |
| `chatModify`, app-state sync | done (live) | rantai hash cocok dengan snapshot server |
| Grup (CRUD, peserta, invite, setelan, approval, foto, label anggota) | done (live) | peserta add/remove perlu akun kedua |
| Komunitas | done (live, opt-in) | `leave` oleh pemilik tidak berefek |
| Newsletter/channel (termasuk deskripsi, foto, subscriber) | done (live) | |
| Status/story | done (live) | area `status`: teks, link, gambar ke diri sendiri, tersimpan lokal, lalu dihapus |
| Privasi, blocklist, profil, mode disappearing default | done (live) | |
| Kontak (`addOrEditContact`/`removeContact`) | done (live) | operasi REMOVE; encoder diperbaiki |
| Quick reply, profil bisnis, detail order | done (unit) | butuh akun WhatsApp Business / token order; tidak diuji live |
| `requestPlaceholderResend` | done (live: diterima server) | |
| Pairing code, QR, multi-device, LID, usync | done | |
| Panggilan | done | Baileys hanya punya `rejectCall`; tidak ada `offer` |
| Bot/AI | partial | `send_bot_prompt`, `get_bot_list_v2` |

## Binding

| Binding | Status | Catatan |
|---|---|---|
| napi | done (unit) | event: semua event core muncul di `sock.ev` dengan nama/bentuk Baileys. Native: setiap method core terjangkau (test Rust). Socket JS: setiap kunci `makeWASocket` Baileys 7.0.0-rc14 ada di `sock` atau dikecualikan dengan alasan (test `node --test`); 162 kunci. `index.d.ts` belum diperiksa `tsc` |
| ffi (uniffi) | done (unit) | semua event core diteruskan; setiap method core terjangkau atau dikecualikan dengan alasan, dijaga test `parity_tests` (2026-10-23). Swift/Kotlin hasil generate belum dikompilasi di sini (tanpa swiftc/kotlinc) |
| cabi | done (unit) | callback event (`baileys_client_set_event_callback`, JSON serde `BaileysEvent`), setiap method core terjangkau atau dikecualikan dengan alasan (test), header dijaga test terhadap ekspor, `baileys_last_error()`. Terbukti dengan program C yang di-link ke `.so` (2026-10-23) |

`CredsUpdate` baru dipancarkan dari loop koneksi (pairing, LID, companion finish); jalur `save_creds` lain di `client/` belum.
Binding: napi mengirim event baru sebagai `{event:"raw"}`; ffi meneruskan semuanya sebagai varian `RolleysEvent` bertipe.

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
Konfirmasi visual (pengguna, 2026-10-23): seluruh pesan `rich` termasuk tombol/list muncul di HP, bukan hanya `server_ack`.

**App-state sync / chatModify: BELUM BERFUNGSI (ditemukan live, 2026-10-23).** Sebelumnya `chat_modify_*` dan label selalu
mengembalikan `Ok` karena kegagalan syncd hanya masuk log debug; sekarang `dispatch_syncd_patch` meneruskan error.
Hasil live (`--only chat`): resync penuh mengembalikan `<collection name=.../>` kosong (tanpa `version`, snapshot, patch)
untuk kelima koleksi, dan setiap patch (mute, pin, archive, mark unread, star) ditolak server `400 bad-request`.
Sudah dicek: request resync dan struktur patch/MAC/versi sama dengan Baileys; `snapshot_blob_ref` memang tidak pernah
diunduh di core, tetapi server di sini tidak mengirim snapshot sama sekali, jadi itu bukan penyebab utama.
Belum diketahui penyebabnya. Dugaan yang belum diuji: perangkat companion ini belum dianggap "sudah sinkron" oleh
server (mis. permintaan kunci/initial sync tidak pernah selesai), atau `index_value_map` tidak disimpan sehingga hash salah
setelah patch pertama. Status: `chatModify`, `labels`, app-state sync = **missing**, bukan done.
Dicoba juga pada grup sungguhan (`--chat-group DXT`, 21 anggota; tanpa kirim pesan/ubah anggota): hasil identik, semua patch
`400 bad-request` dan resync kosong. Jadi penyebabnya bukan karena target self-chat. Tidak ada state yang berubah (semua ditolak).

**App-state sync: akar masalah ditemukan (2026-10-23).** Penyebab `400 bad-request`: encoder mengisi field `version` pada
patch, sedangkan Baileys tidak (server yang menetapkannya). Setelah dihapus, server menerima mute/pin/archive/unread di
grup DXT. Dua cacat lain ditemukan dari membaca sumber Baileys dan diperbaiki: (1) hash LTHash tidak mengurangi nilai lama
untuk indeks yang sama (mute lalu unmute), sehingga snapshot MAC salah di perangkat lain; server tidak bisa mendeteksinya;
(2) `index_value_map` dikosongkan setiap patch terkirim. Juga: `force_snapshot` memakai versi tersimpan sehingga server
membalas kosong dan state yang menyimpang tidak pernah pulih; kini memulai dari state kosong.
Dugaan "kunci bukan yang terbaru" diuji dan **tidak** terbukti (dibatalkan).
Catatan: patch uji yang terkirim SEBELUM perbaikan hash (pasangan aksi/pemulihan pada indeks yang sama) membawa snapshot MAC
yang salah; dampaknya di HP belum diverifikasi. Status chatModify: kode diperbaiki dan teruji unit melawan decoder, bukti live
dengan hash benar masih menunggu.

**App-state sync: terverifikasi live (2026-10-23).** Koreksi atas catatan sebelumnya: perbandingan "rantai = snapshot" yang
pertama TIDAK sahih, karena resync tidak pernah mengunduh snapshot eksternal (`snapshot_blob_ref`) dan berhenti setelah satu
putaran meski `has_more_patches=true`; kedua sisi dibangun dari basis yang sama-sama kurang (hanya ~124 dari 1724 mutasi).
Kini: snapshot dan `external_mutations` diunduh (`MediaType::AppState`, HKDF "WhatsApp App State Keys"), resync mengulang
sampai tidak ada patch lagi, kegagalan satu koleksi tidak disimpan sebagian, dan snapshot/patch divalidasi MAC-nya terhadap
server. Hasil: 5 koleksi, 1724 mutasi, tanpa peringatan; chatModify (mute/pin/archive/unread/star) diterima server dan
state lokal hasil rantai patch identik dengan rekonstruksi snapshot (regular_low v123, 800 entri). Status: done (live).
Catatan: patch yang dikirim sebelum perbaikan ini memakai hash dari basis yang tidak lengkap; pengguna melaporkan grup
uji normal, tetapi dampak jangka panjang di perangkat lain tidak dapat diverifikasi dari sini.

**Event dari app-state (2026-10-23).** Aksi syncd kini dikenali berdasarkan field nilai yang terisi (seperti
`processSyncAction` Baileys): kunci chat, setting (locale, timeFormat, unarchiveChats, statusPrivacy, disableLinkPreviews,
privacySettingRelayAllCalls, notificationActivitySetting, channelsPersonalisedRecommendation), pemetaan LID, kontak LID,
hapus-untuk-saya, dan bintang. `chats.delete`/`messages.delete` tidak dipancarkan saat sinkronisasi awal (seperti Baileys).
**Bug ditemukan:** kanal event berkapasitas 256 membuang 1509 dari ~1766 event saat satu resync; kapasitas kini 16384
(~3,5 MiB per klien, 224 B per slot) dengan test. Referensi lokal: Baileys 7.0.0-rc14 `0af2386`, zapo `c02165a`.
**Pertanyaan terbuka:** koleksi untuk `star` berbeda antar referensi (Baileys `regular_low`, zapo `regular_high`); dibiarkan
`regular_low` karena uji live diterima dan rantai hash cocok dengan snapshot.

`chats.upsert` di Baileys hanya dipancarkan pada notifikasi pembuatan grup; chat dari history sync masuk ke
`messaging-history.set` (padanan: `HistorySync`). `messages.reaction`: pengirim bisa berupa LID (`...@lid`), sesuai yang
dikirim server; pemetaan ke nomor belum dilakukan di event ini.

**messaging-history.set lengkap (2026-10-23).** `HistorySync` kini membawa payload Baileys: `contacts`, `messages` (rata),
`lid_pn_mappings` (termasuk turunan dari chat dan fallback `userReceipt`), `past_participants`, `is_latest`, `chunk_order`,
`peer_data_request_session_id`; setiap chat hanya menyimpan pesan terbarunya (`history.rs`, teruji unit).
**Bug ditemukan:** `fetch_message_history` mengirim IQ `w:sync:history` yang tidak ada di Baileys, jadi riwayat on-demand
tidak pernah berfungsi. Kini memakai pesan peer `PEER_DATA_OPERATION_REQUEST_MESSAGE` ke HP sendiri (`send_peer_message`:
hanya device 0, dibungkus deviceSentMessage, `category=peer`, `push_priority=high_force`, `<meta appdata=default/>`, tanpa
tctoken), dienkripsi lewat sesi LID; versi awal lewat sesi PN membuat balasan HP gagal didekripsi (MAC).
`oldest_timestamp` = detik, dikirim sebagai milidetik.
Belum terbukti live: jalur history sync awal/RECENT/PUSH_NAME (hanya saat pairing) dan `is_latest=true`.
**Pertanyaan terbuka:** on-demand untuk self-chat mengembalikan chat dengan 0 pesan (alamat PN maupun LID); area `history`
melewatinya tanpa `--chat-group`.

**Tiga event terakhir (2026-10-23).** Notifikasi `mex` kini ditangani (`MessageCappingInfoNotification`); op `mex` lain
(reachout timelock, update newsletter lama) belum menghasilkan event. Status history mengikuti Baileys: `complete` untuk
INITIAL_BOOTSTRAP pertama dan RECENT progress 100, `paused` setelah 120 dtk tanpa potongan RECENT (pelacak murni + timer
per koneksi). Ketiganya baru teruji unit: capping didorong server, status history hanya saat pairing, dan tag anggota
butuh perubahan label oleh seseorang di grup.

**Celah socket napi vs Baileys (fitur yang belum ada di core):** addOrEditContact, removeContact, addOrEditQuickReply,
removeQuickReply, getOrderDetails, updateBussinesProfile, updateMemberLabel, updateDefaultDisappearingMode,
fetchDisappearingDuration, requestPlaceholderResend, newsletterUpdate, newsletterUpdateDescription, newsletterUpdatePicture,
newsletterRemovePicture, newsletterSubscribers; updateCoverPhoto (core menerima foto yang sudah diunggah). Daftar ini
dijaga oleh `bindings/baileys-napi/test/sock-api.test.js` (NOT_EXPOSED).

**cabi (2026-10-23).** Sebelumnya pemanggil C tidak bisa menerima event sama sekali dan 100 method core tidak terjangkau
(termasuk `group_create`, `send_reaction`, `send_read_receipt`, `me_jid`, `is_connected`, `ping`). Kini 99 fungsi dihasilkan
dari satu tabel (`scripts/gen-cabi-parity.py`) dan callback event tersedia. Test kontrak menemukan 5 ekspor lama yang tidak
pernah dideklarasikan di `baileys.h` (`send_json`, `send_event`, `send_status_image`, `send_status_video`,
`recognize_message_json`). Contoh C di `FFI_GUIDE.md` memakai fungsi yang tidak ada; sudah ditulis ulang dan terkompilasi.

**Celah terakhir ditutup (2026-10-23).** Ditambahkan di core: kontak, quick reply, `read_messages` batch, `update_member_label`,
`request_placeholder_resend`, mode disappearing default (set + baca), detail order, profil bisnis, serta `newsletter_update`
(deskripsi, foto, hapus foto) dan `newsletter_subscribers`. Temuan dari uji live: (1) encoder syncd salah untuk operasi
REMOVE (mengurangi MAC baru, bukan nilai lama); (2) `newsletter_query_metadata` tidak membaca foto karena server mengirim
`direct_path`, bukan `url`; (3) id query SUBSCRIBERS milik Baileys dijawab server dengan objek admin, jadi jumlah subscriber
diambil dari metadata channel; (4) `update_profile_picture` tidak mengecilkan/memotong gambar seperti Baileys, kini ya
(persegi 640 px JPEG); (5) `send_read_receipt` tidak menyertakan atribut `t`.
Dua id query newsletter Baileys (UPDATE_METADATA dan lainnya) berbeda dari yang dipakai core; yang dipakai core terbukti
bekerja live, jadi tidak diubah.

**Regresi penuh (2026-10-23):** `live-suite` tanpa filter area, akun 6283162498175: 77 lulus, 0 gagal, 3 dilewati
(`history` butuh `--chat-group`; peserta grup butuh akun kedua; komunitas opt-in karena tak bisa dihapus lewat API).
Belum terbukti live: history sync awal (`INITIAL_BOOTSTRAP`/`RECENT`/`PUSH_NAME`, `is_latest=true`,
`messaging-history.status`), karena hanya terjadi saat pairing perangkat baru.
