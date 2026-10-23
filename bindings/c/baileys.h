/*
 * Rolleys C-ABI Header (bindings/c/baileys.h)
 * Multiplatform C/C++ interface for Rolleys WhatsApp engine.
 * Compatible with Windows, Linux, macOS, Android, and iOS.
 */

#ifndef ROLLEYS_H
#define ROLLEYS_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32) || defined(__CYGWIN__)
  #if defined(BAILEYS_BUILD_DLL)
    #define BAILEYS_API __declspec(dllexport)
  #elif defined(BAILEYS_USE_DLL)
    #define BAILEYS_API __declspec(dllimport)
  #else
    #define BAILEYS_API
  #endif
#elif defined(__GNUC__) && __GNUC__ >= 4
  #define BAILEYS_API __attribute__((visibility("default")))
#else
  #define BAILEYS_API
#endif

/**
 * Opaque handle representing an active BaileysClient instance and its Tokio async runtime.
 */
typedef struct BaileysClientHandle BaileysClientHandle;

/* ========================================================================== */
/* Lifecycle & Core Connection                                               */
/* ========================================================================== */

BAILEYS_API BaileysClientHandle* baileys_client_create_sqlite(const char* db_path);
BAILEYS_API BaileysClientHandle* baileys_client_create_with_url(const char* db_path, const char* ws_url);
BAILEYS_API int baileys_client_connect(BaileysClientHandle* handle);

/**
 * Receives every client event as JSON: the serde form of the core `BaileysEvent`. Unit events are
 * bare JSON strings ("Connecting", "CredsUpdate"); the others are objects keyed by the event name
 * with snake_case fields, e.g. {"QrCode":{"code":"..."}} or {"ChatsLock":{"jid":"...","locked":true}}.
 *
 * `event_json` is only valid during the call; copy it to keep it. The callback runs on a runtime
 * worker thread, so `user_data` must be safe to use from another thread, and the callback must not
 * call baileys_client_set_event_callback, baileys_client_destroy or other blocking
 * baileys_client_* functions on the same handle.
 */
typedef void (*BaileysEventCallback)(const char* event_json, void* user_data);

/**
 * Registers `callback` (NULL stops delivery), replacing any previous one. When this returns the
 * previous callback is no longer running and will not be called again. Events raised earlier are
 * not replayed, so register before baileys_client_connect to see the QR/pairing code.
 * Returns 0 on success, -1 for a NULL handle.
 */
BAILEYS_API int baileys_client_set_event_callback(BaileysClientHandle* handle, BaileysEventCallback callback, void* user_data);
BAILEYS_API void baileys_client_disconnect(BaileysClientHandle* handle);
BAILEYS_API void baileys_client_destroy(BaileysClientHandle* handle);
BAILEYS_API void baileys_free_string(char* s);

/* Describes an encoded WAProto Message (as JSON: its recognised kind and summary). Needs no client.
 * Returns NULL for invalid input. */
BAILEYS_API char* baileys_client_recognize_message_json(const uint8_t* proto_bytes, size_t proto_len);

/* ========================================================================== */
/* Messaging                                                                  */
/* ========================================================================== */

BAILEYS_API char* baileys_client_send_text(BaileysClientHandle* handle, const char* to_jid, const char* text);
BAILEYS_API char* baileys_client_edit_message(BaileysClientHandle* handle, const char* to_jid, const char* message_id, const char* new_text);
BAILEYS_API char* baileys_client_revoke_message(BaileysClientHandle* handle, const char* to_jid, const char* message_id);
BAILEYS_API char* baileys_client_send_location(BaileysClientHandle* handle, const char* to_jid, double lat, double lon, const char* name, const char* address);
BAILEYS_API char* baileys_client_send_contact(BaileysClientHandle* handle, const char* to_jid, const char* display_name, const char* vcard);
BAILEYS_API char* baileys_client_send_group_invite(BaileysClientHandle* handle, const char* to_jid, const char* group_jid, const char* group_name, const char* invite_code, int64_t exp_sec, const char* caption);
BAILEYS_API char* baileys_client_send_voice_note(BaileysClientHandle* handle, const char* to_jid, const uint8_t* ptt_bytes, size_t len, uint32_t seconds);
BAILEYS_API char* baileys_client_send_document(BaileysClientHandle* handle, const char* to_jid, const uint8_t* bytes, size_t len, const char* filename, const char* mimetype);
BAILEYS_API char* baileys_client_send_video(BaileysClientHandle* handle, const char* to_jid, const uint8_t* bytes, size_t len, const char* caption, bool is_gif);
BAILEYS_API char* baileys_client_send_sticker(BaileysClientHandle* handle, const char* to_jid, const uint8_t* webp_bytes, size_t len, bool is_animated);
/* Sends any message described as JSON (text, media shorthands, quoted, ...); returns the message id. */
BAILEYS_API char* baileys_client_send_json(BaileysClientHandle* handle, const char* to_jid, const char* json_str);
/* Sends a calendar event. `description` and `location` may be NULL; `end_time` <= 0 means no end. */
BAILEYS_API char* baileys_client_send_event(BaileysClientHandle* handle, const char* to_jid, const char* name, const char* description, int64_t start_time, int64_t end_time, const char* location);

/* ========================================================================== */
/* Polls & Interactive Messages                                               */
/* ========================================================================== */

BAILEYS_API char* baileys_client_send_poll(BaileysClientHandle* handle, const char* to_jid, const char* name, const char* const* options, size_t option_count, uint32_t selectable_count);
BAILEYS_API char* baileys_client_send_poll_vote(BaileysClientHandle* handle, const char* to_jid, const char* poll_msg_id, const char* const* option_hashes, size_t count);
BAILEYS_API char* baileys_client_get_poll_tally(BaileysClientHandle* handle, const char* poll_msg_id);
BAILEYS_API char* baileys_client_send_interactive_buttons(BaileysClientHandle* handle, const char* to_jid, const char* body, const char* footer, const char* json_buttons);
BAILEYS_API char* baileys_client_send_list_message(BaileysClientHandle* handle, const char* to_jid, const char* title, const char* desc, const char* button_text, const char* json_sections);

/* ========================================================================== */
/* Chats & Ephemeral Timers                                                   */
/* ========================================================================== */

BAILEYS_API int baileys_client_chat_mute(BaileysClientHandle* handle, const char* jid, int64_t mute_until);
BAILEYS_API int baileys_client_chat_pin(BaileysClientHandle* handle, const char* jid, bool pin);
BAILEYS_API int baileys_client_chat_archive(BaileysClientHandle* handle, const char* jid, bool archive);
BAILEYS_API int baileys_client_chat_star(BaileysClientHandle* handle, const char* jid, const char* msg_id, bool star);
BAILEYS_API int baileys_client_chat_clear(BaileysClientHandle* handle, const char* jid);
BAILEYS_API int baileys_client_chat_delete(BaileysClientHandle* handle, const char* jid);
BAILEYS_API int baileys_client_chat_mark_unread(BaileysClientHandle* handle, const char* jid, bool unread);
BAILEYS_API int baileys_client_set_chat_ephemeral(BaileysClientHandle* handle, const char* jid, uint32_t expiration_secs);
BAILEYS_API int32_t baileys_client_get_chat_ephemeral(BaileysClientHandle* handle, const char* jid);
BAILEYS_API char* baileys_client_fetch_message_history(BaileysClientHandle* handle, const char* jid, uint32_t limit, int64_t before_ts);

/* ========================================================================== */
/* Groups & Communities                                                       */
/* ========================================================================== */

BAILEYS_API char* baileys_client_group_fetch_metadata(BaileysClientHandle* handle, const char* group_jid);
BAILEYS_API char* baileys_client_group_fetch_all_participating(BaileysClientHandle* handle);
BAILEYS_API int baileys_client_group_join_approval_mode(BaileysClientHandle* handle, const char* group_jid, bool enabled);
BAILEYS_API char* baileys_client_group_request_participants_list(BaileysClientHandle* handle, const char* group_jid);
BAILEYS_API char* baileys_client_group_request_participants_update(BaileysClientHandle* handle, const char* group_jid, const char* const* user_jids, size_t count, const char* action);
BAILEYS_API int baileys_client_group_member_add_mode(BaileysClientHandle* handle, const char* group_jid, bool admin_only);
BAILEYS_API int baileys_client_group_revoke_invite_v4(BaileysClientHandle* handle, const char* group_jid, const char* user_jid);
BAILEYS_API int baileys_client_group_accept_invite_v4(BaileysClientHandle* handle, const char* group_jid, const char* invite_code, int64_t exp);
BAILEYS_API char* baileys_client_community_create(BaileysClientHandle* handle, const char* name, const char* desc);
BAILEYS_API char* baileys_client_community_create_subgroup(BaileysClientHandle* handle, const char* parent_community_jid, const char* name);
BAILEYS_API int baileys_client_community_link_subgroup(BaileysClientHandle* handle, const char* parent_community_jid, const char* sub_group_jid);
BAILEYS_API int baileys_client_community_unlink_subgroup(BaileysClientHandle* handle, const char* parent_community_jid, const char* sub_group_jid);
BAILEYS_API char* baileys_client_community_fetch_subgroups(BaileysClientHandle* handle, const char* parent_community_jid);
BAILEYS_API char* baileys_client_community_get_metadata(BaileysClientHandle* handle, const char* parent_community_jid);
BAILEYS_API int baileys_client_community_leave(BaileysClientHandle* handle, const char* parent_community_jid);
BAILEYS_API char* baileys_client_community_get_invite_code(BaileysClientHandle* handle, const char* parent_community_jid);
BAILEYS_API char* baileys_client_community_accept_invite(BaileysClientHandle* handle, const char* invite_code);

/* ========================================================================== */
/* Channels / Newsletters                                                     */
/* ========================================================================== */

BAILEYS_API char* baileys_client_newsletter_query_metadata(BaileysClientHandle* handle, const char* jid_or_invite);
BAILEYS_API char* baileys_client_newsletter_get_subscribed(BaileysClientHandle* handle);
BAILEYS_API int baileys_client_newsletter_follow(BaileysClientHandle* handle, const char* jid);
BAILEYS_API int baileys_client_newsletter_unfollow(BaileysClientHandle* handle, const char* jid);
BAILEYS_API int baileys_client_newsletter_mute(BaileysClientHandle* handle, const char* jid, bool mute);
BAILEYS_API char* baileys_client_newsletter_send_post(BaileysClientHandle* handle, const char* jid, const char* text);
BAILEYS_API int baileys_client_newsletter_react(BaileysClientHandle* handle, const char* jid, int64_t server_id, const char* reaction);

/* ========================================================================== */
/* Profile, Privacy & Presence                                                */
/* ========================================================================== */

BAILEYS_API char* baileys_client_profile_picture_url(BaileysClientHandle* handle, const char* jid, bool high_res);
BAILEYS_API int baileys_client_presence_subscribe(BaileysClientHandle* handle, const char* jid);
BAILEYS_API char* baileys_client_fetch_privacy_settings(BaileysClientHandle* handle);
BAILEYS_API int baileys_client_update_privacy_setting(BaileysClientHandle* handle, const char* name, const char* value);
BAILEYS_API char* baileys_client_fetch_blocklist(BaileysClientHandle* handle);
BAILEYS_API char* baileys_client_get_blocked_contacts(BaileysClientHandle* handle);
BAILEYS_API int baileys_client_update_block_status(BaileysClientHandle* handle, const char* jid, const char* action);
BAILEYS_API int baileys_client_update_profile_status(BaileysClientHandle* handle, const char* status_text);
BAILEYS_API char* baileys_client_fetch_status(BaileysClientHandle* handle, const char* jid);
BAILEYS_API char* baileys_client_get_business_profile(BaileysClientHandle* handle, const char* jid);
BAILEYS_API char* baileys_client_on_whatsapp(BaileysClientHandle* handle, const char* const* phones, size_t count);

/* ========================================================================== */
/* Status Stories & Calls                                                     */
/* ========================================================================== */

BAILEYS_API char* baileys_client_send_text_status(BaileysClientHandle* handle, const char* text, uint32_t bg_color, uint32_t font);
/* Posts an image/video status. `caption` and `mimetype` may be NULL; the bytes must be non-empty. */
BAILEYS_API char* baileys_client_send_status_image(BaileysClientHandle* handle, const uint8_t* image_bytes, size_t image_len, const char* caption, const char* mimetype);
BAILEYS_API char* baileys_client_send_status_video(BaileysClientHandle* handle, const uint8_t* video_bytes, size_t video_len, const char* caption, const char* mimetype);
BAILEYS_API int baileys_client_send_status_read_receipt(BaileysClientHandle* handle, const char* story_msg_id, const char* author_jid);
BAILEYS_API char* baileys_client_get_recent_stories(BaileysClientHandle* handle, uint32_t limit, bool unviewed_only);
BAILEYS_API char* baileys_client_get_stories_by_author(BaileysClientHandle* handle, const char* author_jid);
BAILEYS_API int baileys_client_reject_call(BaileysClientHandle* handle, const char* call_id, const char* call_from);
BAILEYS_API char* baileys_client_get_call_history(BaileysClientHandle* handle, uint32_t limit);

/* ========================================================================== */
/* Labels & Catalog/Commerce                                                  */
/* ========================================================================== */

BAILEYS_API char* baileys_client_get_labels(BaileysClientHandle* handle);
BAILEYS_API char* baileys_client_get_label(BaileysClientHandle* handle, const char* label_id);
BAILEYS_API int baileys_client_upsert_label(BaileysClientHandle* handle, const char* label_id, const char* name, uint32_t color, int32_t predefined_id);
BAILEYS_API int baileys_client_delete_label(BaileysClientHandle* handle, const char* label_id);
BAILEYS_API int baileys_client_add_chat_label(BaileysClientHandle* handle, const char* chat_jid, const char* label_id);
BAILEYS_API int baileys_client_remove_chat_label(BaileysClientHandle* handle, const char* chat_jid, const char* label_id);
BAILEYS_API char* baileys_client_get_chat_labels(BaileysClientHandle* handle, const char* chat_jid);
BAILEYS_API char* baileys_client_get_chats_by_label(BaileysClientHandle* handle, const char* label_id);
BAILEYS_API int baileys_client_add_message_label(BaileysClientHandle* handle, const char* chat_jid, const char* message_id, const char* label_id);
BAILEYS_API int baileys_client_remove_message_label(BaileysClientHandle* handle, const char* chat_jid, const char* message_id, const char* label_id);
BAILEYS_API char* baileys_client_get_message_labels(BaileysClientHandle* handle, const char* chat_jid, const char* message_id);
BAILEYS_API char* baileys_client_get_messages_by_label(BaileysClientHandle* handle, const char* label_id);

BAILEYS_API char* baileys_client_get_catalog(BaileysClientHandle* handle, const char* jid, uint32_t limit);
BAILEYS_API char* baileys_client_get_product(BaileysClientHandle* handle, const char* jid, const char* product_id);
BAILEYS_API char* baileys_client_get_collections(BaileysClientHandle* handle, const char* jid, uint32_t limit);
BAILEYS_API char* baileys_client_send_product(BaileysClientHandle* handle, const char* to_jid, const char* owner_jid, const char* product_id, const char* title, const char* desc, const char* currency, int64_t price, const char* image_url);
BAILEYS_API char* baileys_client_send_order(BaileysClientHandle* handle, const char* to_jid, const char* order_id, const char* title, int64_t total_price, const char* currency, uint32_t item_count, const char* status);
BAILEYS_API char* baileys_client_get_cached_orders(BaileysClientHandle* handle, const char* jid);
BAILEYS_API char* baileys_client_get_cached_catalog(BaileysClientHandle* handle, const char* jid);
BAILEYS_API char* baileys_client_get_cached_collections(BaileysClientHandle* handle, const char* jid);

/* ========================================================================== */
/* Companion Device Management (USync)                                        */
/* ========================================================================== */

BAILEYS_API char* baileys_client_get_user_devices(BaileysClientHandle* handle, const char* const* user_jids, size_t count);
BAILEYS_API char* baileys_client_get_paired_devices(BaileysClientHandle* handle);
BAILEYS_API int baileys_client_remove_companion_device(BaileysClientHandle* handle, uint32_t device_id);

/* BEGIN GENERATED (scripts/gen-cabi-parity.py) */

/* ========================================================================== */
/* Parity with the core client (generated)                                    */
/* ========================================================================== */
/*
 * Conventions for every function in this block:
 *  - the first parameter is the client handle. A NULL handle or an invalid argument
 *    (NULL where a string is required, bytes with a NULL pointer, malformed JSON) is
 *    reported through baileys_last_error() and the function returns its failure value;
 *  - int results: 0 success, -1 invalid argument or handle, -2 the operation failed;
 *  - char* results are owned by the caller (release with baileys_free_string) and NULL on
 *    failure; structured results are JSON text;
 *  - optional numbers are int64_t, where a negative value means "not set";
 *  - optional strings and byte buffers are NULL when not set;
 *  - calls block until the server answers, so do not call them from an event callback.
 */

/* Why the last call in this block failed on this thread, or NULL when it did not. Each function in
 * this block resets it on entry. The string is valid until the next call on this thread. */
BAILEYS_API const char* baileys_last_error(void);

/* Whether the client currently has an open session. */
BAILEYS_API bool baileys_client_is_connected(BaileysClientHandle* handle);
/* Whether ghost mode (no read receipts or presence) is on. */
BAILEYS_API bool baileys_client_is_ghost_mode(BaileysClientHandle* handle);
/* Turns ghost mode on or off. */
BAILEYS_API int baileys_client_set_ghost_mode(BaileysClientHandle* handle, bool enabled);
/* Whether incoming media is downloaded automatically. */
BAILEYS_API bool baileys_client_is_auto_download(BaileysClientHandle* handle);
/* Turns automatic media download on or off. */
BAILEYS_API int baileys_client_set_auto_download(BaileysClientHandle* handle, bool enabled);
/* Connection state name, e.g. "Connected". */
BAILEYS_API char* baileys_client_get_connection_state(BaileysClientHandle* handle);
/* Health and traffic counters as JSON. */
BAILEYS_API char* baileys_client_telemetry_snapshot(BaileysClientHandle* handle);
/* Our own JID, or NULL when not logged in. */
BAILEYS_API char* baileys_client_me_jid(BaileysClientHandle* handle);
/* Our own LID, or NULL when unknown. */
BAILEYS_API char* baileys_client_me_lid(BaileysClientHandle* handle);
/* Round-trip time to the server in milliseconds, -1 on failure. */
BAILEYS_API int64_t baileys_client_ping(BaileysClientHandle* handle);
/* Logs this device out and clears the session. */
BAILEYS_API int baileys_client_logout(BaileysClientHandle* handle);
/* The most recent QR string, or NULL. */
BAILEYS_API char* baileys_client_get_last_qr(BaileysClientHandle* handle);
/* Requests an 8-character pairing code for the phone number. */
BAILEYS_API char* baileys_client_request_pairing_code(BaileysClientHandle* handle, const char* phone);
/* Re-synchronises app-state collections; returns a JSON summary. */
BAILEYS_API char* baileys_client_resync_app_state(BaileysClientHandle* handle, const char* const* collections, size_t collections_count, bool force_snapshot);
/* Changes our push name. */
BAILEYS_API int baileys_client_update_profile_name(BaileysClientHandle* handle, const char* name);
/* Resolves a WhatsApp username; JSON object, or the string null when not found. */
BAILEYS_API char* baileys_client_lookup_username(BaileysClientHandle* handle, const char* username, const char* pin);
/* Maps phone-number JIDs to LIDs; JSON array of [pn, lid]. */
BAILEYS_API char* baileys_client_get_lid_for_pn(BaileysClientHandle* handle, const char* const* pn_jids, size_t pn_jids_count);
/* Maps LIDs to phone-number JIDs; JSON array of [lid, pn]. */
BAILEYS_API char* baileys_client_get_pn_for_lid(BaileysClientHandle* handle, const char* const* lid_jids, size_t lid_jids_count);
/* About text of several users; JSON array of [jid, status, set_at]. */
BAILEYS_API char* baileys_client_get_user_status(BaileysClientHandle* handle, const char* const* jids, size_t jids_count);
/* State of Terms-of-Service notices as JSON. */
BAILEYS_API char* baileys_client_query_tos(BaileysClientHandle* handle, const char* const* notice_ids, size_t notice_ids_count);
/* Accepts Terms-of-Service notices. */
BAILEYS_API int baileys_client_accept_tos(BaileysClientHandle* handle, const char* const* notice_ids, size_t notice_ids_count, const char* stage);
/* The linked recovery email, or NULL. */
BAILEYS_API char* baileys_client_get_linked_email(BaileysClientHandle* handle);
/* Sets the recovery email. */
BAILEYS_API int baileys_client_set_linked_email(BaileysClientHandle* handle, const char* email);
/* Asks for an email verification code. */
BAILEYS_API int baileys_client_request_email_code(BaileysClientHandle* handle, const char* lang, const char* locale);
/* Confirms the email verification code. */
BAILEYS_API int baileys_client_verify_email_code(BaileysClientHandle* handle, const char* code);
/* Clears a server dirty bit (account_sync or groups). */
BAILEYS_API int baileys_client_clean_dirty_bits(BaileysClientHandle* handle, const char* dirty_type, int64_t from_timestamp);
/* Reachout time-lock state as JSON [active, type, ends_at], or null. */
BAILEYS_API char* baileys_client_fetch_account_reachout_timelock(BaileysClientHandle* handle);
/* New-chat message cap as JSON, or null. */
BAILEYS_API char* baileys_client_fetch_new_chat_message_cap(BaileysClientHandle* handle);
/* Available Meta AI bots; JSON array of [jid, name]. */
BAILEYS_API char* baileys_client_get_bot_list_v2(BaileysClientHandle* handle);
/* Creates a call link; NULL when the server returns none. */
BAILEYS_API char* baileys_client_create_call_link(BaileysClientHandle* handle, bool is_video);
/* Reacts to a message; empty emoji removes the reaction. */
BAILEYS_API char* baileys_client_send_reaction(BaileysClientHandle* handle, const char* to_jid, const char* target_msg_id, const char* emoji);
/* Replies to a message. */
BAILEYS_API char* baileys_client_send_text_message_with_quote(BaileysClientHandle* handle, const char* to_jid, const char* text, const char* quoted_id, const char* quoted_participant, const char* quoted_text);
/* Sends text with a link preview. */
BAILEYS_API char* baileys_client_send_text_with_preview(BaileysClientHandle* handle, const char* to_jid, const char* text);
/* Replies to a quote that was never sent. */
BAILEYS_API char* baileys_client_send_fake_quoted_message(BaileysClientHandle* handle, const char* to_jid, const char* reply_text, const char* fake_sender_jid, const char* fake_quoted_text);
/* Deletes a message for everyone. */
BAILEYS_API char* baileys_client_send_revoke_message(BaileysClientHandle* handle, const char* to_jid, const char* target_msg_id);
/* Pins or unpins a message. */
BAILEYS_API char* baileys_client_pin_message(BaileysClientHandle* handle, const char* chat_jid, const char* msg_id, bool pin, uint32_t duration_secs);
/* Forwards a message this client has seen, by its id. */
BAILEYS_API char* baileys_client_forward_message(BaileysClientHandle* handle, const char* to_jid, const char* message_id);
/* Comments on a message. */
BAILEYS_API char* baileys_client_send_comment(BaileysClientHandle* handle, const char* to_jid, const char* target_msg_id, bool target_from_me, const char* target_participant, const char* text);
/* Sends an album header for the media that follows. */
BAILEYS_API char* baileys_client_send_album(BaileysClientHandle* handle, const char* to_jid, uint32_t expected_image_count, uint32_t expected_video_count);
/* Shares a live location. */
BAILEYS_API char* baileys_client_send_live_location(BaileysClientHandle* handle, const char* to_jid, double latitude, double longitude, int64_t accuracy_in_meters, const char* caption);
/* Sends a contact card from vCard text. */
BAILEYS_API char* baileys_client_send_vcard(BaileysClientHandle* handle, const char* to_jid, const char* display_name, const char* vcard);
/* Sends several contacts; JSON array of [name, phone]. */
BAILEYS_API char* baileys_client_send_contacts_array(BaileysClientHandle* handle, const char* to_jid, const char* display_name, const char* contacts);
/* Sends a JSON-described message as view-once. */
BAILEYS_API char* baileys_client_send_view_once(BaileysClientHandle* handle, const char* to_jid, const char* message_json, bool v2);
/* Sends a prompt to Meta AI. */
BAILEYS_API char* baileys_client_send_bot_prompt(BaileysClientHandle* handle, const char* prompt, const char* target_chat);
/* Marks a message as read. */
BAILEYS_API int baileys_client_send_read_receipt(BaileysClientHandle* handle, const char* remote_jid, const char* msg_id, const char* participant);
/* Sends presence (available, composing, ...). */
BAILEYS_API int baileys_client_send_presence_update(BaileysClientHandle* handle, const char* presence, const char* to_jid);
/* Sends an image. */
BAILEYS_API char* baileys_client_send_image_message(BaileysClientHandle* handle, const char* to_jid, const uint8_t* image, size_t image_len, const char* caption);
/* Sends audio, as a voice note when ptt is true. */
BAILEYS_API char* baileys_client_send_audio_message(BaileysClientHandle* handle, const char* to_jid, const uint8_t* audio, size_t audio_len, bool ptt, int64_t seconds);
/* Sends a video. */
BAILEYS_API char* baileys_client_send_video_message(BaileysClientHandle* handle, const char* to_jid, const uint8_t* video, size_t video_len, const char* caption, int64_t seconds, bool gif_playback);
/* Sends a document. */
BAILEYS_API char* baileys_client_send_document_message(BaileysClientHandle* handle, const char* to_jid, const uint8_t* document, size_t document_len, const char* file_name, const char* mimetype, const char* title);
/* Sends a WebP sticker. */
BAILEYS_API char* baileys_client_send_sticker_message(BaileysClientHandle* handle, const char* to_jid, const uint8_t* sticker, size_t sticker_len, bool is_animated);
/* Streams a file from disk (image, video, audio, document, sticker). */
BAILEYS_API char* baileys_client_send_media_file_streaming(BaileysClientHandle* handle, const char* chat_jid, const char* file_path, const char* media_type, const char* caption);
/* Link preview of the first URL in text, as JSON or null. */
BAILEYS_API char* baileys_client_fetch_url_preview(BaileysClientHandle* handle, const char* text);
/* Asks the phone to refresh expired media. */
BAILEYS_API int baileys_client_update_media_message(BaileysClientHandle* handle, const char* message_id);
/* Downloads message media into a directory; returns the file path. */
BAILEYS_API char* baileys_client_download_media_for_message(BaileysClientHandle* handle, const char* msg_id, const char* dest_dir);
/* Downloads message media to a path (or the default one); returns the file path. */
BAILEYS_API char* baileys_client_download_message_media_to_path(BaileysClientHandle* handle, const char* msg_id, const char* target_path);
/* Locally stored chats as JSON. */
BAILEYS_API char* baileys_client_get_chats(BaileysClientHandle* handle);
/* Locally stored messages of a chat as JSON. */
BAILEYS_API char* baileys_client_get_messages(BaileysClientHandle* handle, const char* chat_jid, uint32_t limit, int64_t before_timestamp);
/* A locally stored contact as JSON, or null. */
BAILEYS_API char* baileys_client_get_contact(BaileysClientHandle* handle, const char* jid);
/* Sets a profile picture (ours when jid is NULL). */
BAILEYS_API int baileys_client_update_profile_picture(BaileysClientHandle* handle, const char* jid, const uint8_t* image, size_t image_len);
/* Removes a profile picture (ours when jid is NULL). */
BAILEYS_API int baileys_client_remove_profile_picture(BaileysClientHandle* handle, const char* jid);
/* Sets the business cover photo from an uploaded photo id. */
BAILEYS_API int baileys_client_update_cover_photo(BaileysClientHandle* handle, const char* photo_id, const char* token, uint64_t ts);
/* Removes the business cover photo. */
BAILEYS_API int baileys_client_remove_cover_photo(BaileysClientHandle* handle, const char* photo_id);
/* Creates a group; returns its JID. */
BAILEYS_API char* baileys_client_group_create(BaileysClientHandle* handle, const char* subject, const char* const* participants, size_t participants_count);
/* Group metadata as JSON. */
BAILEYS_API char* baileys_client_group_metadata(BaileysClientHandle* handle, const char* group_jid);
/* Metadata of the group behind an invite code, without joining. */
BAILEYS_API char* baileys_client_group_get_invite_info(BaileysClientHandle* handle, const char* code);
/* add/remove/promote/demote; JSON array of [jid, status]. */
BAILEYS_API char* baileys_client_group_participants_update(BaileysClientHandle* handle, const char* group_jid, const char* const* participants, size_t participants_count, const char* action);
/* Renames a group. */
BAILEYS_API int baileys_client_group_update_subject(BaileysClientHandle* handle, const char* group_jid, const char* subject);
/* Sets a group description. */
BAILEYS_API int baileys_client_group_update_description(BaileysClientHandle* handle, const char* group_jid, const char* description);
/* announcement, not_announcement, locked or unlocked. */
BAILEYS_API int baileys_client_group_setting_update(BaileysClientHandle* handle, const char* group_jid, const char* setting);
/* Current invite code of a group. */
BAILEYS_API char* baileys_client_group_get_invite_code(BaileysClientHandle* handle, const char* group_jid);
/* Revokes the invite code; returns the new one. */
BAILEYS_API char* baileys_client_group_revoke_invite(BaileysClientHandle* handle, const char* group_jid);
/* Joins a group by invite code; returns its JID. */
BAILEYS_API char* baileys_client_group_accept_invite(BaileysClientHandle* handle, const char* code);
/* Leaves a group. */
BAILEYS_API int baileys_client_group_leave(BaileysClientHandle* handle, const char* group_jid);
/* JIDs of the communities we are in. */
BAILEYS_API char* baileys_client_community_fetch_all_participating(BaileysClientHandle* handle);
/* Community behind an invite code. */
BAILEYS_API char* baileys_client_community_get_invite_info(BaileysClientHandle* handle, const char* invite_code);
/* Revokes the invite code; returns the new one. */
BAILEYS_API char* baileys_client_community_revoke_invite(BaileysClientHandle* handle, const char* community_jid);
/* Sets the disappearing-messages timer. */
BAILEYS_API int baileys_client_community_toggle_ephemeral(BaileysClientHandle* handle, const char* community_jid, uint32_t duration_sec);
/* announcement, not_announcement, locked or unlocked. */
BAILEYS_API int baileys_client_community_setting_update(BaileysClientHandle* handle, const char* community_jid, const char* setting);
/* admin_add or all_member_add. */
BAILEYS_API int baileys_client_community_member_add_mode(BaileysClientHandle* handle, const char* community_jid, const char* mode);
/* on or off. */
BAILEYS_API int baileys_client_community_join_approval_mode(BaileysClientHandle* handle, const char* community_jid, const char* mode);
/* Creates a channel; returns its metadata. */
BAILEYS_API char* baileys_client_newsletter_create(BaileysClientHandle* handle, const char* name, const char* description);
/* Deletes a channel we own. */
BAILEYS_API int baileys_client_newsletter_delete(BaileysClientHandle* handle, const char* jid);
/* Renames a channel. */
BAILEYS_API int baileys_client_newsletter_update_name(BaileysClientHandle* handle, const char* jid, const char* name);
/* Recent channel posts as JSON. */
BAILEYS_API char* baileys_client_newsletter_fetch_messages(BaileysClientHandle* handle, const char* jid, uint32_t count, int64_t before);
/* Number of channel admins, -1 on failure. */
BAILEYS_API int64_t baileys_client_newsletter_admin_count(BaileysClientHandle* handle, const char* jid);
/* Makes a user a channel admin. */
BAILEYS_API int baileys_client_newsletter_promote(BaileysClientHandle* handle, const char* jid, const char* user_jid);
/* Removes channel admin rights. */
BAILEYS_API int baileys_client_newsletter_demote(BaileysClientHandle* handle, const char* jid, const char* user_jid);
/* Transfers channel ownership. */
BAILEYS_API int baileys_client_newsletter_change_owner(BaileysClientHandle* handle, const char* jid, const char* new_owner_jid);
/* Marks channel posts as viewed. */
BAILEYS_API int baileys_client_newsletter_view_receipt(BaileysClientHandle* handle, const char* jid, const uint64_t* server_ids, size_t server_ids_count);
/* Changes name, description and/or picture (base64 JPEG, empty string removes) in one request. */
BAILEYS_API int baileys_client_newsletter_update(BaileysClientHandle* handle, const char* jid, const char* update);
/* Sets the channel description. */
BAILEYS_API int baileys_client_newsletter_update_description(BaileysClientHandle* handle, const char* jid, const char* description);
/* Sets the channel picture from any image (cropped square, 640 px JPEG). */
BAILEYS_API int baileys_client_newsletter_update_picture(BaileysClientHandle* handle, const char* jid, const uint8_t* image, size_t image_len);
/* Removes the channel picture. */
BAILEYS_API int baileys_client_newsletter_remove_picture(BaileysClientHandle* handle, const char* jid);
/* Subscriber count of a channel, -1 on failure. */
BAILEYS_API int64_t baileys_client_newsletter_subscribers(BaileysClientHandle* handle, const char* jid);
/* Posts an image to a channel. */
BAILEYS_API char* baileys_client_newsletter_send_image(BaileysClientHandle* handle, const char* jid, const uint8_t* image, size_t image_len, const char* caption);
/* Shares a channel post as a status story. */
BAILEYS_API char* baileys_client_newsletter_send_status_story(BaileysClientHandle* handle, const char* newsletter_jid, int32_t server_msg_id, const char* newsletter_name, const char* caption_or_text, const char* const* status_recipients, size_t status_recipients_count);
/* Shares a channel image as a status story. */
BAILEYS_API char* baileys_client_newsletter_send_image_status_story(BaileysClientHandle* handle, const uint8_t* image, size_t image_len, const char* caption, const char* newsletter_jid, int32_t server_msg_id, const char* newsletter_name, const char* const* status_recipients, size_t status_recipients_count);
/* Subscribes to live channel updates; returns the duration or NULL. */
BAILEYS_API char* baileys_client_subscribe_newsletter_updates(BaileysClientHandle* handle, const char* newsletter_jid);
/* Marks many messages read at once; JSON array of {remote_jid, id, participant, from_me}. Uses read-self when read receipts are off. */
BAILEYS_API int baileys_client_read_messages(BaileysClientHandle* handle, const char* keys);
/* Sets our label in a group (30 characters at most). */
BAILEYS_API char* baileys_client_update_member_label(BaileysClientHandle* handle, const char* group_jid, const char* label);
/* Asks our phone to resend a message that arrived undecryptable. */
BAILEYS_API char* baileys_client_request_placeholder_resend(BaileysClientHandle* handle, const char* chat_jid, const char* msg_id, bool from_me, const char* participant);
/* Default disappearing timer for new chats (0 = off). */
BAILEYS_API int baileys_client_update_default_disappearing_mode(BaileysClientHandle* handle, uint32_t duration_secs);
/* Default disappearing timer of users; JSON array of [jid, seconds, set_at]. */
BAILEYS_API char* baileys_client_fetch_disappearing_duration(BaileysClientHandle* handle, const char* const* jids, size_t jids_count);
/* Adds or edits a contact; JSON {full_name, first_name, lid_jid, pn_jid, username, save_on_primary_addressbook}. */
BAILEYS_API int baileys_client_add_or_edit_contact(BaileysClientHandle* handle, const char* jid, const char* contact);
/* Removes a contact (it must exist in the synced app state). */
BAILEYS_API int baileys_client_remove_contact(BaileysClientHandle* handle, const char* jid);
/* Adds or edits a quick reply (WhatsApp Business); returns its id. */
BAILEYS_API char* baileys_client_add_or_edit_quick_reply(BaileysClientHandle* handle, const char* timestamp, const char* shortcut, const char* message);
/* Deletes a quick reply by its id. */
BAILEYS_API int baileys_client_remove_quick_reply(BaileysClientHandle* handle, const char* timestamp);
/* Details of an order, using the token from the order message. */
BAILEYS_API char* baileys_client_get_order_details(BaileysClientHandle* handle, const char* order_id, const char* token_base64);
/* Updates our business profile; JSON {address, email, description, websites, hours}; unset fields stay. */
BAILEYS_API int baileys_client_update_business_profile(BaileysClientHandle* handle, const char* update);
/* Posts an audio status; audience NULL means all contacts. */
BAILEYS_API char* baileys_client_send_status_audio(BaileysClientHandle* handle, const uint8_t* audio, size_t audio_len, uint32_t seconds, const char* mimetype, const char* const* audience, size_t audience_count);
/* Posts a link status. */
BAILEYS_API char* baileys_client_send_link_status(BaileysClientHandle* handle, const char* url, const char* title, const char* description, const char* custom_text, const char* background_color, int64_t font, const uint8_t* thumbnail, size_t thumbnail_len, const char* const* audience, size_t audience_count);
/* Creates a catalog product from JSON; returns its id. */
BAILEYS_API char* baileys_client_product_create(BaileysClientHandle* handle, const char* product);
/* Updates a catalog product from JSON. */
BAILEYS_API int baileys_client_product_update(BaileysClientHandle* handle, const char* product_id, const char* product);
/* Deletes catalog products. */
BAILEYS_API int baileys_client_product_delete(BaileysClientHandle* handle, const char* const* product_ids, size_t product_ids_count);

/* END GENERATED */

#ifdef __cplusplus
}
#endif

#endif /* ROLLEYS_H */
