#!/usr/bin/env python3
"""Generates the C-ABI functions that bring baileys-cabi to parity with the core client.

Writes bindings/baileys-cabi/src/parity.rs and the matching block of bindings/c/baileys.h from the
table below. Both outputs are committed; run this after changing the table:

    python3 scripts/gen-cabi-parity.py

Conventions of every generated function (documented in the header):
  * first parameter is the client handle; a NULL handle or an invalid argument is reported through
    baileys_last_error() and the function returns its failure value;
  * `int` results: 0 success, -1 invalid argument/handle, -2 the operation failed;
  * `char*` results are owned by the caller (free with baileys_free_string), NULL on failure;
    structured results are JSON;
  * optional numeric arguments are `int64_t` where a negative value means "not set".
"""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
RS_OUT = ROOT / "bindings/baileys-cabi/src/parity.rs"
H_OUT = ROOT / "bindings/c/baileys.h"

# Argument kinds -> (C parameter text, doc note)
# s: required string        os: nullable string      b: bool       u32/i32/u64/i64/f64: numbers
# ou32/ou64/oi32: optional number as int64_t (<0 = none)
# bytes/obytes: (ptr,len)   strs/ostrs: (char**,count)   u64s: (uint64_t*,count)
# json:TYPE: JSON text deserialized into TYPE
# Return kinds: unit (int), str, ostr, json (char*), i64 (int64_t), and the sync variants
# bool_sync, unit_sync, ostr_sync, json_sync.

SPEC = [
    # ---- connection & account ----
    ("is_connected", [], "h.client.is_connected()", "bool_sync", "Whether the client currently has an open session."),
    ("is_ghost_mode", [], "h.client.is_ghost_mode()", "bool_sync", "Whether ghost mode (no read receipts or presence) is on."),
    ("set_ghost_mode", [("enabled", "b")], "h.client.set_ghost_mode(enabled)", "unit_sync", "Turns ghost mode on or off."),
    ("is_auto_download", [], "h.client.is_auto_download()", "bool_sync", "Whether incoming media is downloaded automatically."),
    ("set_auto_download", [("enabled", "b")], "h.client.set_auto_download(enabled)", "unit_sync", "Turns automatic media download on or off."),
    ("get_connection_state", [], "Some(format!(\"{:?}\", h.client.get_connection_state()))", "ostr_sync", "Connection state name, e.g. \"Connected\"."),
    ("telemetry_snapshot", [], "serde_json::to_string(&h.client.telemetry_snapshot()).ok()", "ostr_sync", "Health and traffic counters as JSON."),
    ("me_jid", [], "c.me_jid().await", "ostr", "Our own JID, or NULL when not logged in."),
    ("me_lid", [], "c.me_lid().await", "ostr", "Our own LID, or NULL when unknown."),
    ("ping", [], "c.ping().await.map(|d| d.as_millis() as i64)", "i64", "Round-trip time to the server in milliseconds, -1 on failure."),
    ("logout", [], "c.logout().await", "unit", "Logs this device out and clears the session."),
    ("get_last_qr", [], "Ok::<Option<String>, baileys_core::CoreError>(c.get_last_qr().await)", "ostr", "The most recent QR string, or NULL."),
    ("request_pairing_code", [("phone", "s")], "c.request_pairing_code(&phone).await", "str", "Requests an 8-character pairing code for the phone number."),
    ("resync_app_state", [("collections", "strs"), ("force_snapshot", "b")],
     "{ let refs: Vec<&str> = collections.iter().map(String::as_str).collect(); c.resync_app_state(&refs, force_snapshot).await }", "json",
     "Re-synchronises app-state collections; returns a JSON summary."),
    ("update_profile_name", [("name", "s")], "c.update_profile_name(&name).await", "unit", "Changes our push name."),
    ("lookup_username", [("username", "s"), ("pin", "os")],
     "c.lookup_username(&username, pin.as_deref()).await.map(|u| u.map(|u| serde_json::json!({\"username\": u.username, \"jid\": u.jid, \"phone_jid\": u.phone_jid, \"verified_name\": u.verified_name})))", "json",
     "Resolves a WhatsApp username; JSON object, or the string null when not found."),
    ("get_lid_for_pn", [("pn_jids", "strs")],
     "{ let refs: Vec<&str> = pn_jids.iter().map(String::as_str).collect(); c.get_lid_for_pn(&refs).await }", "json", "Maps phone-number JIDs to LIDs; JSON array of [pn, lid]."),
    ("get_pn_for_lid", [("lid_jids", "strs")],
     "{ let refs: Vec<&str> = lid_jids.iter().map(String::as_str).collect(); c.get_pn_for_lid(&refs).await }", "json", "Maps LIDs to phone-number JIDs; JSON array of [lid, pn]."),
    ("get_user_status", [("jids", "strs")],
     "{ let refs: Vec<&str> = jids.iter().map(String::as_str).collect(); c.get_user_status(&refs).await }", "json", "About text of several users; JSON array of [jid, status, set_at]."),
    ("query_tos", [("notice_ids", "strs")],
     "{ let refs: Vec<&str> = notice_ids.iter().map(String::as_str).collect(); c.query_tos(&refs).await }", "json", "State of Terms-of-Service notices as JSON."),
    ("accept_tos", [("notice_ids", "strs"), ("stage", "os")],
     "{ let refs: Vec<&str> = notice_ids.iter().map(String::as_str).collect(); c.accept_tos(&refs, stage.as_deref()).await }", "unit", "Accepts Terms-of-Service notices."),
    ("get_linked_email", [], "c.get_linked_email().await", "ostr", "The linked recovery email, or NULL."),
    ("set_linked_email", [("email", "s")], "c.set_linked_email(&email).await", "unit", "Sets the recovery email."),
    ("request_email_code", [("lang", "s"), ("locale", "s")], "c.request_email_code(&lang, &locale).await", "unit", "Asks for an email verification code."),
    ("verify_email_code", [("code", "s")], "c.verify_email_code(&code).await", "unit", "Confirms the email verification code."),
    ("clean_dirty_bits", [("dirty_type", "s"), ("from_timestamp", "ou64")], "c.clean_dirty_bits(&dirty_type, from_timestamp).await", "unit", "Clears a server dirty bit (account_sync or groups)."),
    ("fetch_account_reachout_timelock", [], "c.fetch_account_reachout_timelock().await", "json", "Reachout time-lock state as JSON [active, type, ends_at], or null."),
    ("fetch_new_chat_message_cap", [], "c.fetch_new_chat_message_cap().await", "json", "New-chat message cap as JSON, or null."),
    ("get_bot_list_v2", [], "c.get_bot_list_v2().await", "json", "Available Meta AI bots; JSON array of [jid, name]."),
    ("create_call_link", [("is_video", "b")], "c.create_call_link(is_video).await", "ostr", "Creates a call link; NULL when the server returns none."),
    # ---- messaging ----
    ("send_reaction", [("to_jid", "s"), ("target_msg_id", "s"), ("emoji", "s")], "c.send_reaction(&to_jid, &target_msg_id, &emoji).await", "str", "Reacts to a message; empty emoji removes the reaction."),
    ("send_text_message_with_quote", [("to_jid", "s"), ("text", "s"), ("quoted_id", "s"), ("quoted_participant", "os"), ("quoted_text", "os")],
     "c.send_text_message_with_quote(&to_jid, &text, &quoted_id, quoted_participant.as_deref(), quoted_text.as_deref()).await", "str", "Replies to a message."),
    ("send_text_with_preview", [("to_jid", "s"), ("text", "s")], "c.send_text_with_preview(&to_jid, &text).await", "str", "Sends text with a link preview."),
    ("send_fake_quoted_message", [("to_jid", "s"), ("reply_text", "s"), ("fake_sender_jid", "s"), ("fake_quoted_text", "s")],
     "c.send_fake_quoted_message(&to_jid, &reply_text, &fake_sender_jid, &fake_quoted_text).await", "str", "Replies to a quote that was never sent."),
    ("send_revoke_message", [("to_jid", "s"), ("target_msg_id", "s")], "c.send_revoke_message(&to_jid, &target_msg_id).await", "str", "Deletes a message for everyone."),
    ("pin_message", [("chat_jid", "s"), ("msg_id", "s"), ("pin", "b"), ("duration_secs", "u32")], "c.pin_message(&chat_jid, &msg_id, pin, duration_secs).await", "str", "Pins or unpins a message."),
    ("forward_message", [("to_jid", "s"), ("message_id", "s")],
     "{ match c.get_recent_message(&message_id).await.and_then(|m| m.message) { Some(m) => c.forward_message(&to_jid, m, true, 1).await, None => { use prost::Message as _; match c.store.get_message(&message_id).await { Ok(Some(s)) => match baileys_core::baileys_proto::proto::Message::decode(s.proto_bytes.as_slice()) { Ok(m) => c.forward_message(&to_jid, m, true, 1).await, Err(e) => Err(baileys_core::CoreError::Protocol(e.to_string())) }, Ok(None) => Err(baileys_core::CoreError::Protocol(format!(\"message {message_id} not found\"))), Err(e) => Err(baileys_core::CoreError::Store(e.to_string())) } } } }", "str",
     "Forwards a message this client has seen, by its id."),
    ("send_comment", [("to_jid", "s"), ("target_msg_id", "s"), ("target_from_me", "b"), ("target_participant", "os"), ("text", "s")],
     "{ let key = baileys_core::baileys_proto::proto::MessageKey { remote_jid: Some(to_jid.clone()), from_me: Some(target_from_me), id: Some(target_msg_id), participant: target_participant }; c.send_comment(&to_jid, &key, &text).await }", "str", "Comments on a message."),
    ("send_album", [("to_jid", "s"), ("expected_image_count", "u32"), ("expected_video_count", "u32")], "c.send_album(&to_jid, expected_image_count, expected_video_count).await", "str", "Sends an album header for the media that follows."),
    ("send_live_location", [("to_jid", "s"), ("latitude", "f64"), ("longitude", "f64"), ("accuracy_in_meters", "ou32"), ("caption", "os")],
     "c.send_live_location(&to_jid, latitude, longitude, accuracy_in_meters, caption.as_deref()).await", "str", "Shares a live location."),
    ("send_vcard", [("to_jid", "s"), ("display_name", "s"), ("vcard", "s")], "c.send_vcard(&to_jid, &display_name, &vcard).await", "str", "Sends a contact card from vCard text."),
    ("send_contacts_array", [("to_jid", "s"), ("display_name", "s"), ("contacts", "json:Vec<(String, String)>")],
     "c.send_contacts_array(&to_jid, &display_name, contacts).await", "str", "Sends several contacts; JSON array of [name, phone]."),
    ("send_view_once", [("to_jid", "s"), ("message_json", "s"), ("v2", "b")],
     "{ match serde_json::from_str::<serde_json::Value>(&message_json).map_err(|e| e.to_string()).and_then(|v| baileys_core::messages::parse_message_from_json(&v)) { Ok(inner) => c.send_view_once(&to_jid, inner, v2).await, Err(e) => Err(baileys_core::CoreError::Protocol(e)) } }", "str",
     "Sends a JSON-described message as view-once."),
    ("send_bot_prompt", [("prompt", "s"), ("target_chat", "os")], "c.send_bot_prompt(&prompt, target_chat.as_deref()).await", "str", "Sends a prompt to Meta AI."),
    ("send_read_receipt", [("remote_jid", "s"), ("msg_id", "s"), ("participant", "os")], "c.send_read_receipt(&remote_jid, &msg_id, participant.as_deref()).await", "unit", "Marks a message as read."),
    ("send_presence_update", [("presence", "s"), ("to_jid", "os")], "c.send_presence_update(&presence, to_jid.as_deref()).await", "unit", "Sends presence (available, composing, ...)."),
    ("send_image_message", [("to_jid", "s"), ("image", "bytes"), ("caption", "os")], "c.send_image_message(&to_jid, &image, caption.as_deref()).await", "str", "Sends an image."),
    ("send_audio_message", [("to_jid", "s"), ("audio", "bytes"), ("ptt", "b"), ("seconds", "ou32")], "c.send_audio_message(&to_jid, &audio, ptt, seconds).await", "str", "Sends audio, as a voice note when ptt is true."),
    ("send_video_message", [("to_jid", "s"), ("video", "bytes"), ("caption", "os"), ("seconds", "ou32"), ("gif_playback", "b")],
     "c.send_video_message(&to_jid, &video, caption.as_deref(), seconds, gif_playback).await", "str", "Sends a video."),
    ("send_document_message", [("to_jid", "s"), ("document", "bytes"), ("file_name", "s"), ("mimetype", "s"), ("title", "os")],
     "c.send_document_message(&to_jid, &document, &file_name, &mimetype, title.as_deref()).await", "str", "Sends a document."),
    ("send_sticker_message", [("to_jid", "s"), ("sticker", "bytes"), ("is_animated", "b")], "c.send_sticker_message(&to_jid, &sticker, is_animated).await", "str", "Sends a WebP sticker."),
    ("send_media_file_streaming", [("chat_jid", "s"), ("file_path", "s"), ("media_type", "s"), ("caption", "os")],
     "{ match media_type.parse::<baileys_media::MediaType>() { Ok(t) => c.send_media_file_streaming(&chat_jid, std::path::Path::new(&file_path), t, caption.as_deref()).await, Err(e) => Err(baileys_core::CoreError::Media(e.to_string())) } }", "str",
     "Streams a file from disk (image, video, audio, document, sticker)."),
    ("fetch_url_preview", [("text", "s")], "c.fetch_url_preview(&text).await", "json", "Link preview of the first URL in text, as JSON or null."),
    ("update_media_message", [("message_id", "s")], "c.update_media_message(&message_id).await", "unit", "Asks the phone to refresh expired media."),
    ("download_media_for_message", [("msg_id", "s"), ("dest_dir", "s")],
     "c.download_media_for_message(&msg_id, std::path::Path::new(&dest_dir)).await.map(|p| p.to_string_lossy().into_owned())", "str", "Downloads message media into a directory; returns the file path."),
    ("download_message_media_to_path", [("msg_id", "s"), ("target_path", "os")],
     "c.download_message_media_to_path(&msg_id, target_path.as_deref().map(std::path::Path::new)).await.map(|p| p.to_string_lossy().into_owned())", "str", "Downloads message media to a path (or the default one); returns the file path."),
    ("get_chats", [], "c.get_chats().await", "json", "Locally stored chats as JSON."),
    ("get_messages", [("chat_jid", "s"), ("limit", "u32"), ("before_timestamp", "ou64")], "c.get_messages(&chat_jid, limit, before_timestamp).await", "json", "Locally stored messages of a chat as JSON."),
    ("get_contact", [("jid", "s")], "c.get_contact(&jid).await", "json", "A locally stored contact as JSON, or null."),
    # ---- profile ----
    ("update_profile_picture", [("jid", "os"), ("image", "bytes")], "c.update_profile_picture(jid.as_deref(), &image).await", "unit", "Sets a profile picture (ours when jid is NULL)."),
    ("remove_profile_picture", [("jid", "os")], "c.remove_profile_picture(jid.as_deref()).await", "unit", "Removes a profile picture (ours when jid is NULL)."),
    ("update_cover_photo", [("photo_id", "s"), ("token", "s"), ("ts", "u64")], "c.update_cover_photo(&photo_id, &token, ts).await", "unit", "Sets the business cover photo from an uploaded photo id."),
    ("remove_cover_photo", [("photo_id", "s")], "c.remove_cover_photo(&photo_id).await", "unit", "Removes the business cover photo."),
    # ---- groups ----
    ("group_create", [("subject", "s"), ("participants", "strs")], "c.group_create(&subject, &participants).await", "str", "Creates a group; returns its JID."),
    ("group_metadata", [("group_jid", "s")],
     "c.group_metadata(&group_jid).await.and_then(|n| baileys_core::groups::parse_group_metadata(&n))", "json", "Group metadata as JSON."),
    ("group_get_invite_info", [("code", "s")],
     "c.group_get_invite_info(&code).await.and_then(|n| baileys_core::groups::parse_group_metadata(&n))", "json", "Metadata of the group behind an invite code, without joining."),
    ("group_participants_update", [("group_jid", "s"), ("participants", "strs"), ("action", "s")],
     "c.group_participants_update(&group_jid, &participants, &action).await", "json", "add/remove/promote/demote; JSON array of [jid, status]."),
    ("group_update_subject", [("group_jid", "s"), ("subject", "s")], "c.group_update_subject(&group_jid, &subject).await", "unit", "Renames a group."),
    ("group_update_description", [("group_jid", "s"), ("description", "s")], "c.group_update_description(&group_jid, &description).await", "unit", "Sets a group description."),
    ("group_setting_update", [("group_jid", "s"), ("setting", "s")], "c.group_setting_update(&group_jid, &setting).await", "unit", "announcement, not_announcement, locked or unlocked."),
    ("group_get_invite_code", [("group_jid", "s")], "c.group_get_invite_code(&group_jid).await", "str", "Current invite code of a group."),
    ("group_revoke_invite", [("group_jid", "s")], "c.group_revoke_invite(&group_jid).await", "str", "Revokes the invite code; returns the new one."),
    ("group_accept_invite", [("code", "s")], "c.group_accept_invite(&code).await", "str", "Joins a group by invite code; returns its JID."),
    ("group_leave", [("group_jid", "s")], "c.group_leave(&group_jid).await", "unit", "Leaves a group."),
    # ---- communities ----
    ("community_fetch_all_participating", [], "c.community_fetch_all_participating().await", "json", "JIDs of the communities we are in."),
    ("community_get_invite_info", [("invite_code", "s")], "c.community_get_invite_info(&invite_code).await", "json", "Community behind an invite code."),
    ("community_revoke_invite", [("community_jid", "s")], "c.community_revoke_invite(&community_jid).await", "str", "Revokes the invite code; returns the new one."),
    ("community_toggle_ephemeral", [("community_jid", "s"), ("duration_sec", "u32")], "c.community_toggle_ephemeral(&community_jid, duration_sec).await", "unit", "Sets the disappearing-messages timer."),
    ("community_setting_update", [("community_jid", "s"), ("setting", "s")], "c.community_setting_update(&community_jid, &setting).await", "unit", "announcement, not_announcement, locked or unlocked."),
    ("community_member_add_mode", [("community_jid", "s"), ("mode", "s")], "c.community_member_add_mode(&community_jid, &mode).await", "unit", "admin_add or all_member_add."),
    ("community_join_approval_mode", [("community_jid", "s"), ("mode", "s")], "c.community_join_approval_mode(&community_jid, &mode).await", "unit", "on or off."),
    # ---- newsletters ----
    ("newsletter_create", [("name", "s"), ("description", "os")], "c.newsletter_create(&name, description.as_deref()).await", "json", "Creates a channel; returns its metadata."),
    ("newsletter_delete", [("jid", "s")], "c.newsletter_delete(&jid).await", "unit", "Deletes a channel we own."),
    ("newsletter_update_name", [("jid", "s"), ("name", "s")], "c.newsletter_update_name(&jid, &name).await", "unit", "Renames a channel."),
    ("newsletter_fetch_messages", [("jid", "s"), ("count", "u32"), ("before", "ou64")], "c.newsletter_fetch_messages(&jid, count, before).await", "json", "Recent channel posts as JSON."),
    ("newsletter_admin_count", [("jid", "s")], "c.newsletter_admin_count(&jid).await.map(|n| n as i64)", "i64", "Number of channel admins, -1 on failure."),
    ("newsletter_promote", [("jid", "s"), ("user_jid", "s")], "c.newsletter_promote(&jid, &user_jid).await", "unit", "Makes a user a channel admin."),
    ("newsletter_demote", [("jid", "s"), ("user_jid", "s")], "c.newsletter_demote(&jid, &user_jid).await", "unit", "Removes channel admin rights."),
    ("newsletter_change_owner", [("jid", "s"), ("new_owner_jid", "s")], "c.newsletter_change_owner(&jid, &new_owner_jid).await", "unit", "Transfers channel ownership."),
    ("newsletter_view_receipt", [("jid", "s"), ("server_ids", "u64s")], "c.newsletter_view_receipt(&jid, &server_ids).await", "unit", "Marks channel posts as viewed."),
    ("newsletter_update", [("jid", "s"), ("update", "json:baileys_core::newsletter::NewsletterUpdate")], "c.newsletter_update(&jid, &update).await", "unit",
     "Changes name, description and/or picture (base64 JPEG, empty string removes) in one request."),
    ("newsletter_update_description", [("jid", "s"), ("description", "s")], "c.newsletter_update_description(&jid, &description).await", "unit", "Sets the channel description."),
    ("newsletter_update_picture", [("jid", "s"), ("image", "bytes")], "c.newsletter_update_picture(&jid, &image).await", "unit", "Sets the channel picture from any image (cropped square, 640 px JPEG)."),
    ("newsletter_remove_picture", [("jid", "s")], "c.newsletter_remove_picture(&jid).await", "unit", "Removes the channel picture."),
    ("newsletter_subscribers", [("jid", "s")], "c.newsletter_subscribers(&jid).await.map(|n| n as i64)", "i64", "Subscriber count of a channel, -1 on failure."),
    ("newsletter_send_image", [("jid", "s"), ("image", "bytes"), ("caption", "os")], "c.newsletter_send_image(&jid, &image, caption.as_deref()).await", "str", "Posts an image to a channel."),
    ("newsletter_send_status_story", [("newsletter_jid", "s"), ("server_msg_id", "i32"), ("newsletter_name", "s"), ("caption_or_text", "s"), ("status_recipients", "ostrs")],
     "c.newsletter_send_status_story(&newsletter_jid, server_msg_id, &newsletter_name, &caption_or_text, status_recipients).await", "str", "Shares a channel post as a status story."),
    ("newsletter_send_image_status_story", [("image", "bytes"), ("caption", "os"), ("newsletter_jid", "s"), ("server_msg_id", "i32"), ("newsletter_name", "s"), ("status_recipients", "ostrs")],
     "c.newsletter_send_image_status_story(&image, caption.as_deref(), &newsletter_jid, server_msg_id, &newsletter_name, status_recipients).await", "str", "Shares a channel image as a status story."),
    ("subscribe_newsletter_updates", [("newsletter_jid", "s")], "c.subscribe_newsletter_updates(&newsletter_jid).await", "ostr", "Subscribes to live channel updates; returns the duration or NULL."),
    ("read_messages", [("keys", "json:Vec<baileys_core::receipts::ReadKey>")], "c.read_messages(&keys).await", "unit",
     "Marks many messages read at once; JSON array of {remote_jid, id, participant, from_me}. Uses read-self when read receipts are off."),
    ("update_member_label", [("group_jid", "s"), ("label", "s")], "c.update_member_label(&group_jid, &label).await", "str", "Sets our label in a group (30 characters at most)."),
    ("request_placeholder_resend", [("chat_jid", "s"), ("msg_id", "s"), ("from_me", "b"), ("participant", "os")],
     "{ let key = baileys_core::baileys_proto::proto::MessageKey { remote_jid: Some(chat_jid), from_me: Some(from_me), id: Some(msg_id), participant }; c.request_placeholder_resend(&key).await }", "str",
     "Asks our phone to resend a message that arrived undecryptable."),
    ("update_default_disappearing_mode", [("duration_secs", "u32")], "c.update_default_disappearing_mode(duration_secs).await", "unit", "Default disappearing timer for new chats (0 = off)."),
    ("fetch_disappearing_duration", [("jids", "strs")],
     "{ let refs: Vec<&str> = jids.iter().map(String::as_str).collect(); c.fetch_disappearing_duration(&refs).await }", "json", "Default disappearing timer of users; JSON array of [jid, seconds, set_at]."),
    ("add_or_edit_contact", [("jid", "s"), ("contact", "json:baileys_core::syncd::ContactEdit")], "c.add_or_edit_contact(&jid, &contact).await", "unit",
     "Adds or edits a contact; JSON {full_name, first_name, lid_jid, pn_jid, username, save_on_primary_addressbook}."),
    ("remove_contact", [("jid", "s")], "c.remove_contact(&jid).await", "unit", "Removes a contact (it must exist in the synced app state)."),
    ("add_or_edit_quick_reply", [("timestamp", "os"), ("shortcut", "s"), ("message", "s")], "c.add_or_edit_quick_reply(timestamp.as_deref(), &shortcut, &message).await", "str",
     "Adds or edits a quick reply (WhatsApp Business); returns its id."),
    ("remove_quick_reply", [("timestamp", "s")], "c.remove_quick_reply(&timestamp).await", "unit", "Deletes a quick reply by its id."),
    ("get_order_details", [("order_id", "s"), ("token_base64", "s")], "c.get_order_details(&order_id, &token_base64).await", "json", "Details of an order, using the token from the order message."),
    ("update_business_profile", [("update", "json:baileys_core::commerce::BusinessProfileUpdate")], "c.update_business_profile(&update).await", "unit",
     "Updates our business profile; JSON {address, email, description, websites, hours}; unset fields stay."),
    # ---- status ----
    ("send_status_audio", [("audio", "bytes"), ("seconds", "u32"), ("mimetype", "os"), ("audience", "ostrs")],
     "c.send_status_audio(&audio, seconds, mimetype.as_deref(), audience).await", "str", "Posts an audio status; audience NULL means all contacts."),
    ("send_link_status", [("url", "s"), ("title", "os"), ("description", "os"), ("custom_text", "os"), ("background_color", "os"), ("font", "oi32"), ("thumbnail", "obytes"), ("audience", "ostrs")],
     "c.send_link_status(&url, title.as_deref(), description.as_deref(), custom_text.as_deref(), background_color.as_deref(), font, thumbnail, audience).await", "str", "Posts a link status."),
    # ---- commerce ----
    ("product_create", [("product", "json:baileys_core::CatalogProduct")], "c.product_create(&product).await", "str", "Creates a catalog product from JSON; returns its id."),
    ("product_update", [("product_id", "s"), ("product", "json:baileys_core::CatalogProduct")], "c.product_update(&product_id, &product).await", "unit", "Updates a catalog product from JSON."),
    ("product_delete", [("product_ids", "strs")],
     "{ let refs: Vec<&str> = product_ids.iter().map(String::as_str).collect(); c.product_delete(&refs).await }", "unit", "Deletes catalog products."),
]

FAIL = {"unit": "-1", "str": "std::ptr::null_mut()", "ostr": "std::ptr::null_mut()", "json": "std::ptr::null_mut()",
        "i64": "-1", "bool_sync": "false", "unit_sync": "-1", "ostr_sync": "std::ptr::null_mut()", "json_sync": "std::ptr::null_mut()"}
RUST_RET = {"unit": "c_int", "str": "*mut c_char", "ostr": "*mut c_char", "json": "*mut c_char", "i64": "i64",
            "bool_sync": "bool", "unit_sync": "c_int", "ostr_sync": "*mut c_char", "json_sync": "*mut c_char"}
C_RET = {"unit": "int", "str": "char*", "ostr": "char*", "json": "char*", "i64": "int64_t",
         "bool_sync": "bool", "unit_sync": "int", "ostr_sync": "char*", "json_sync": "char*"}


def rust_params(args):
    out = []
    for n, k in args:
        if k in ("s", "os") or k.startswith("json:"):
            out.append(f"{n}: *const c_char")
        elif k in ("bytes", "obytes"):
            out.append(f"{n}: *const u8")
            out.append(f"{n}_len: usize")
        elif k in ("strs", "ostrs"):
            out.append(f"{n}: *const *const c_char")
            out.append(f"{n}_count: usize")
        elif k == "u64s":
            out.append(f"{n}: *const u64")
            out.append(f"{n}_count: usize")
        elif k in ("ou32", "ou64", "oi32"):
            out.append(f"{n}: i64")
        else:  # b, u32, i32, u64, i64, f64
            out.append(f"{n}: {'bool' if k == 'b' else k}")
    return ", ".join(out)


def c_params(args):
    out = []
    for n, k in args:
        if k in ("s", "os") or k.startswith("json:"):
            out.append(f"const char* {n}")
        elif k in ("bytes", "obytes"):
            out.append(f"const uint8_t* {n}, size_t {n}_len")
        elif k in ("strs", "ostrs"):
            out.append(f"const char* const* {n}, size_t {n}_count")
        elif k == "u64s":
            out.append(f"const uint64_t* {n}, size_t {n}_count")
        elif k in ("ou32", "ou64", "oi32"):
            out.append(f"int64_t {n}")
        else:
            ctype = {"b": "bool", "u32": "uint32_t", "i32": "int32_t", "u64": "uint64_t", "i64": "int64_t", "f64": "double"}[k]
            out.append(f"{ctype} {n}")
    return ", ".join(out)


def conversions(args, fail):
    lines = []
    bad = lambda n: f"{{ invalid_arg(\"{n}\"); return {fail}; }}"
    for n, k in args:
        if k == "s":
            lines.append(f"    let {n} = match cstr_opt({n}) {{ Some(Some(v)) => v, _ => {bad(n)} }};")
        elif k == "os":
            lines.append(f"    let {n} = match cstr_opt({n}) {{ Some(v) => v, None => {bad(n)} }};")
        elif k.startswith("json:"):
            ty = k[5:]
            lines.append(
                f"    let {n}: {ty} = match cstr_opt({n}) {{ Some(Some(j)) => match serde_json::from_str(&j) {{ Ok(v) => v, Err(e) => {{ set_last_error(format!(\"{n}: {{e}}\")); return {fail}; }} }}, _ => {bad(n)} }};")
        elif k == "bytes":
            lines.append(f"    let {n} = match byte_slice({n}, {n}_len) {{ Some(v) => v.to_vec(), None => {bad(n)} }};")
        elif k == "obytes":
            lines.append(f"    let {n} = if {n}.is_null() {{ None }} else {{ match byte_slice({n}, {n}_len) {{ Some(v) => Some(v.to_vec()), None => {bad(n)} }} }};")
        elif k == "strs":
            lines.append(f"    let {n} = match str_array({n}, {n}_count) {{ Some(v) => v, None => {bad(n)} }};")
        elif k == "ostrs":
            lines.append(f"    let {n} = if {n}.is_null() {{ None }} else {{ match str_array({n}, {n}_count) {{ Some(v) => Some(v), None => {bad(n)} }} }};")
        elif k == "u64s":
            lines.append(f"    let {n} = match u64_slice({n}, {n}_count) {{ Some(v) => v.to_vec(), None => {bad(n)} }};")
        elif k == "ou32":
            lines.append(f"    let {n} = if {n} < 0 {{ None }} else {{ Some({n} as u32) }};")
        elif k == "ou64":
            lines.append(f"    let {n} = if {n} < 0 {{ None }} else {{ Some({n} as u64) }};")
        elif k == "oi32":
            lines.append(f"    let {n} = if {n} < 0 {{ None }} else {{ Some({n} as i32) }};")
    return "\n".join(lines)


def gen_rust():
    out = ['''// @generated by scripts/gen-cabi-parity.py from its SPEC table. Do not edit by hand.
//
// C-ABI functions for the core client methods the hand-written part of this crate lacks.
// Conventions: see the header block in bindings/c/baileys.h.
#![allow(clippy::too_many_arguments, clippy::missing_safety_doc)]

use crate::{
    byte_slice, cstr_opt, finish_i64, finish_json, finish_ostr, finish_str, finish_unit,
    handle_ref, invalid_arg, set_last_error, str_array, to_c, u64_slice, BaileysClientHandle,
};
use std::os::raw::{c_char, c_int};
''']
    for name, args, body, ret, _doc in SPEC:
        fail = FAIL[ret]
        params = rust_params(args)
        sig_params = "handle: *mut BaileysClientHandle" + (", " + params if params else "")
        sync = ret.endswith("_sync")
        fn = [f"#[no_mangle]", f"pub unsafe extern \"C\" fn baileys_client_{name}({sig_params}) -> {RUST_RET[ret]} {{",
              f"    let Some(h) = handle_ref(handle) else {{ return {fail}; }};"]
        conv = conversions(args, fail)
        if conv:
            fn.append(conv)
        if sync:
            if ret == "bool_sync":
                fn.append(f"    {body}")
            elif ret == "unit_sync":
                fn.append(f"    {body};\n    0")
            else:  # ostr_sync
                fn.append(f"    match {body} {{ Some(s) => to_c(s), None => std::ptr::null_mut() }}")
        else:
            fn.append("    let c = h.client.clone();")
            fn.append(f"    let res = h.runtime.block_on(async move {{ {body} }});")
            finish = {"unit": "finish_unit", "str": "finish_str", "ostr": "finish_ostr", "json": "finish_json", "i64": "finish_i64"}[ret]
            fn.append(f"    {finish}(res)")
        fn.append("}\n")
        out.append("\n".join(fn))
    return "\n".join(out)


BEGIN = "/* BEGIN GENERATED (scripts/gen-cabi-parity.py) */"
END = "/* END GENERATED */"


def gen_header_block():
    lines = [BEGIN, "",
             "/* ========================================================================== */",
             "/* Parity with the core client (generated)                                    */",
             "/* ========================================================================== */",
             "/*",
             " * Conventions for every function in this block:",
             " *  - the first parameter is the client handle. A NULL handle or an invalid argument",
             " *    (NULL where a string is required, bytes with a NULL pointer, malformed JSON) is",
             " *    reported through baileys_last_error() and the function returns its failure value;",
             " *  - int results: 0 success, -1 invalid argument or handle, -2 the operation failed;",
             " *  - char* results are owned by the caller (release with baileys_free_string) and NULL on",
             " *    failure; structured results are JSON text;",
             " *  - optional numbers are int64_t, where a negative value means \"not set\";",
             " *  - optional strings and byte buffers are NULL when not set;",
             " *  - calls block until the server answers, so do not call them from an event callback.",
             " */",
             "",
             "/* Why the last call in this block failed on this thread, or NULL when it did not. Each function in",
             " * this block resets it on entry. The string is valid until the next call on this thread. */",
             "BAILEYS_API const char* baileys_last_error(void);",
             ""]
    for name, args, _body, ret, doc in SPEC:
        params = c_params(args)
        lines.append(f"/* {doc} */")
        lines.append(f"BAILEYS_API {C_RET[ret]} baileys_client_{name}(BaileysClientHandle* handle{', ' + params if params else ''});")
    lines += ["", END]
    return "\n".join(lines)


def main():
    RS_OUT.write_text(gen_rust())
    h = H_OUT.read_text()
    block = gen_header_block()
    if BEGIN in h:
        h = re.sub(re.escape(BEGIN) + r".*?" + re.escape(END), lambda _m: block, h, flags=re.S)
    else:
        marker = "#ifdef __cplusplus\n}\n#endif"
        assert marker in h, "header end marker not found"
        h = h.replace(marker, block + "\n\n" + marker)
    H_OUT.write_text(h)
    print(f"generated {len(SPEC)} functions")


if __name__ == "__main__":
    main()
