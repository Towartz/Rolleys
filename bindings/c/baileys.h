/*
 * Rose-Baileys C-ABI Header (bindings/c/baileys.h)
 * Multiplatform C/C++ interface for Rose-Baileys WhatsApp engine.
 * Compatible with Windows, Linux, macOS, Android, and iOS.
 */

#ifndef ROSE_BAILEYS_H
#define ROSE_BAILEYS_H

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
BAILEYS_API void baileys_client_disconnect(BaileysClientHandle* handle);
BAILEYS_API void baileys_client_destroy(BaileysClientHandle* handle);
BAILEYS_API void baileys_free_string(char* s);

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

#ifdef __cplusplus
}
#endif

#endif /* ROSE_BAILEYS_H */
