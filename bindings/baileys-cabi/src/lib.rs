#![allow(clippy::missing_safety_doc)]

use baileys_core::{BaileysClient, BaileysConfigBuilder};
use baileys_store::SqliteStore;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::Arc;
use tokio::runtime::Runtime;

pub struct BaileysClientHandle {
    client: BaileysClient,
    runtime: Runtime,
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_create_with_url(
    db_path: *const c_char,
    ws_url: *const c_char,
) -> *mut BaileysClientHandle {
    if db_path.is_null() {
        return std::ptr::null_mut();
    }

    let path_str = match CStr::from_ptr(db_path).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let ws_url_str = if !ws_url.is_null() {
        match CStr::from_ptr(ws_url).to_str() {
            Ok(s) => Some(s),
            Err(_) => return std::ptr::null_mut(),
        }
    } else {
        None
    };

    let runtime = match Runtime::new() {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    let store = match SqliteStore::open(path_str) {
        Ok(s) => Arc::new(s),
        Err(_) => return std::ptr::null_mut(),
    };

    let mut builder =
        BaileysConfigBuilder::new().with_browser("Rose-CABI", "Multiplatform", "0.1.0");

    if let Some(url) = ws_url_str {
        builder = builder.with_websocket_url(url);
    }

    let config = builder.build();
    let client = BaileysClient::new(store, config);

    let handle = Box::new(BaileysClientHandle { client, runtime });
    Box::into_raw(handle)
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_create_sqlite(
    db_path: *const c_char,
) -> *mut BaileysClientHandle {
    baileys_client_create_with_url(db_path, std::ptr::null())
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_connect(handle: *mut BaileysClientHandle) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let h = &*handle;
    match h.runtime.block_on(h.client.connect()) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_text(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    text: *const c_char,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || text.is_null() {
        return std::ptr::null_mut();
    }

    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let content = match CStr::from_ptr(text).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.send_text_message(to, content)) {
        Ok(msg_id) => match CString::new(msg_id) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_disconnect(handle: *mut BaileysClientHandle) {
    if !handle.is_null() {
        let h = &*handle;
        h.runtime.block_on(h.client.disconnect());
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_profile_picture_url(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    ptype: *const c_char,
) -> *mut c_char {
    if handle.is_null() || jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let picture_type = if !ptype.is_null() {
        CStr::from_ptr(ptype).to_str().unwrap_or("preview")
    } else {
        "preview"
    };

    match h
        .runtime
        .block_on(h.client.profile_picture_url(to, picture_type))
    {
        Ok(Some(url)) => match CString::new(url) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_presence_subscribe(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
) -> c_int {
    if handle.is_null() || to_jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h.runtime.block_on(h.client.presence_subscribe(to)) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_chat_mute(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    duration_secs: i64,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let dur_opt = if duration_secs <= 0 {
        None
    } else {
        Some(duration_secs as u64)
    };
    match h
        .runtime
        .block_on(h.client.chat_modify_mute(jid_str, dur_opt))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_chat_pin(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    pin: c_int,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.chat_modify_pin(jid_str, pin != 0))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_chat_archive(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    archive: c_int,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.chat_modify_archive(jid_str, archive != 0))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_chat_star(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    msg_id: *const c_char,
    from_me: c_int,
    star: c_int,
) -> c_int {
    if handle.is_null() || jid.is_null() || msg_id.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let msg_id_str = match CStr::from_ptr(msg_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.chat_modify_star(
        jid_str,
        msg_id_str,
        from_me != 0,
        star != 0,
    )) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_newsletter_query_metadata(
    handle: *mut BaileysClientHandle,
    jid_or_invite: *const c_char,
) -> *mut c_char {
    if handle.is_null() || jid_or_invite.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let input = match CStr::from_ptr(jid_or_invite).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h
        .runtime
        .block_on(h.client.newsletter_query_metadata(input))
    {
        Ok(meta) => {
            if let Ok(json) = serde_json::to_string(&meta) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_newsletter_get_subscribed(
    handle: *mut BaileysClientHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    match h.runtime.block_on(h.client.newsletter_get_subscribed()) {
        Ok(list) => {
            if let Ok(json) = serde_json::to_string(&list) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_newsletter_follow(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.newsletter_follow(jid_str)) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_newsletter_unfollow(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.newsletter_unfollow(jid_str)) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_newsletter_mute(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    mute: c_int,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.newsletter_mute(jid_str, mute != 0))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_newsletter_send_post(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    text: *const c_char,
) -> *mut c_char {
    if handle.is_null() || jid.is_null() || text.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let text_str = match CStr::from_ptr(text).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h
        .runtime
        .block_on(h.client.newsletter_send_post(jid_str, text_str))
    {
        Ok(id) => CString::new(id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_newsletter_react(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    server_id: u64,
    reaction: *const c_char,
) -> c_int {
    if handle.is_null() || jid.is_null() || reaction.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let reaction_str = match CStr::from_ptr(reaction).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.newsletter_react(jid_str, server_id, reaction_str))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_poll(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    question: *const c_char,
    options_json: *const c_char,
    selectable_count: u32,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || question.is_null() || options_json.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let q = match CStr::from_ptr(question).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let opts_str = match CStr::from_ptr(options_json).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let options: Vec<String> = match serde_json::from_str(opts_str) {
        Ok(o) => o,
        Err(_) => return std::ptr::null_mut(),
    };

    match h
        .runtime
        .block_on(h.client.send_poll(to, q, &options, selectable_count))
    {
        Ok(msg_id) => match CString::new(msg_id) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_poll_vote(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    poll_id: *const c_char,
    selected_options_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || poll_id.is_null() || selected_options_json.is_null()
    {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let pid = match CStr::from_ptr(poll_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let opts_str = match CStr::from_ptr(selected_options_json).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let options: Vec<String> = match serde_json::from_str(opts_str) {
        Ok(o) => o,
        Err(_) => return std::ptr::null_mut(),
    };

    match h
        .runtime
        .block_on(h.client.send_poll_vote(to, pid, &options))
    {
        Ok(msg_id) => match CString::new(msg_id) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_poll_tally(
    handle: *mut BaileysClientHandle,
    poll_id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || poll_id.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let pid = match CStr::from_ptr(poll_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.get_poll_tally(pid)) {
        Ok(Some(tally)) => {
            if let Ok(json) = serde_json::to_string(&tally) {
                match CString::new(json) {
                    Ok(c_str) => c_str.into_raw(),
                    Err(_) => std::ptr::null_mut(),
                }
            } else {
                std::ptr::null_mut()
            }
        }
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_interactive_buttons(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    body: *const c_char,
    buttons_json: *const c_char,
    header: *const c_char,
    footer: *const c_char,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || body.is_null() || buttons_json.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let body_str = match CStr::from_ptr(body).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let btns_str = match CStr::from_ptr(buttons_json).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let buttons: Vec<baileys_core::InteractiveButton> = match serde_json::from_str(btns_str) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };

    let header_str = if !header.is_null() {
        CStr::from_ptr(header).to_str().ok()
    } else {
        None
    };

    let footer_str = if !footer.is_null() {
        CStr::from_ptr(footer).to_str().ok()
    } else {
        None
    };

    match h.runtime.block_on(
        h.client
            .send_interactive_buttons(to, body_str, &buttons, header_str, footer_str),
    ) {
        Ok(msg_id) => match CString::new(msg_id) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_list_message(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    title: *const c_char,
    description: *const c_char,
    button_text: *const c_char,
    sections_json: *const c_char,
    footer: *const c_char,
) -> *mut c_char {
    if handle.is_null()
        || to_jid.is_null()
        || title.is_null()
        || description.is_null()
        || button_text.is_null()
        || sections_json.is_null()
    {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let title_str = match CStr::from_ptr(title).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let desc_str = match CStr::from_ptr(description).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let btn_str = match CStr::from_ptr(button_text).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let secs_str = match CStr::from_ptr(sections_json).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let sections: Vec<baileys_core::ListSection> = match serde_json::from_str(secs_str) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let footer_str = if !footer.is_null() {
        CStr::from_ptr(footer).to_str().ok()
    } else {
        None
    };

    match h.runtime.block_on(
        h.client
            .send_list_message(to, title_str, desc_str, btn_str, &sections, footer_str),
    ) {
        Ok(msg_id) => match CString::new(msg_id) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_create(
    handle: *mut BaileysClientHandle,
    subject: *const c_char,
    description: *const c_char,
) -> *mut c_char {
    if handle.is_null() || subject.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let subj = match CStr::from_ptr(subject).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let desc = if !description.is_null() {
        CStr::from_ptr(description).to_str().ok()
    } else {
        None
    };

    match h.runtime.block_on(h.client.community_create(subj, desc)) {
        Ok(meta) => match serde_json::to_string(&meta) {
            Ok(json) => CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_create_subgroup(
    handle: *mut BaileysClientHandle,
    parent_community_jid: *const c_char,
    subject: *const c_char,
    participants_json: *const c_char,
) -> *mut c_char {
    if handle.is_null()
        || parent_community_jid.is_null()
        || subject.is_null()
        || participants_json.is_null()
    {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let parent = match CStr::from_ptr(parent_community_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let subj = match CStr::from_ptr(subject).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let parts_str = match CStr::from_ptr(participants_json).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let participants: Vec<String> = match serde_json::from_str(parts_str) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(
        h.client
            .community_create_subgroup(parent, subj, &participants),
    ) {
        Ok(gid) => CString::new(gid)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_link_subgroup(
    handle: *mut BaileysClientHandle,
    parent_community_jid: *const c_char,
    group_jid: *const c_char,
) -> c_int {
    if handle.is_null() || parent_community_jid.is_null() || group_jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let parent = match CStr::from_ptr(parent_community_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let group = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h
        .runtime
        .block_on(h.client.community_link_subgroup(parent, group))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_unlink_subgroup(
    handle: *mut BaileysClientHandle,
    parent_community_jid: *const c_char,
    group_jid: *const c_char,
) -> c_int {
    if handle.is_null() || parent_community_jid.is_null() || group_jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let parent = match CStr::from_ptr(parent_community_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let group = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h
        .runtime
        .block_on(h.client.community_unlink_subgroup(parent, group))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_fetch_subgroups(
    handle: *mut BaileysClientHandle,
    community_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || community_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let comm = match CStr::from_ptr(community_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.community_fetch_subgroups(comm)) {
        Ok(subgroups) => match serde_json::to_string(&subgroups) {
            Ok(json) => CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_get_metadata(
    handle: *mut BaileysClientHandle,
    community_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || community_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let comm = match CStr::from_ptr(community_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.community_get_metadata(comm)) {
        Ok(meta) => match serde_json::to_string(&meta) {
            Ok(json) => CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_leave(
    handle: *mut BaileysClientHandle,
    community_jid: *const c_char,
) -> c_int {
    if handle.is_null() || community_jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let comm = match CStr::from_ptr(community_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h.runtime.block_on(h.client.community_leave(comm)) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_get_invite_code(
    handle: *mut BaileysClientHandle,
    community_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || community_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let comm = match CStr::from_ptr(community_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.community_get_invite_code(comm)) {
        Ok(code) => CString::new(code)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_community_accept_invite(
    handle: *mut BaileysClientHandle,
    code: *const c_char,
) -> *mut c_char {
    if handle.is_null() || code.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let c = match CStr::from_ptr(code).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.community_accept_invite(c)) {
        Ok(jid) => CString::new(jid)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_reject_call(
    handle: *mut BaileysClientHandle,
    call_id: *const c_char,
    call_from: *const c_char,
) -> c_int {
    if handle.is_null() || call_id.is_null() || call_from.is_null() {
        return -1;
    }
    let h = &*handle;
    let cid = match CStr::from_ptr(call_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let cfrom = match CStr::from_ptr(call_from).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h.runtime.block_on(h.client.reject_call(cid, cfrom)) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_call_history(
    handle: *mut BaileysClientHandle,
    limit: u32,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    match h.runtime.block_on(h.client.get_call_history(limit)) {
        Ok(calls) => {
            let json = serde_json::to_string(&calls).unwrap_or_else(|_| "[]".to_string());
            CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_text_status(
    handle: *mut BaileysClientHandle,
    text: *const c_char,
    bg_color: *const c_char,
    font: c_int,
    audience_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || text.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let text_str = match CStr::from_ptr(text).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let bg_str = if !bg_color.is_null() {
        CStr::from_ptr(bg_color).to_str().ok()
    } else {
        None
    };
    let font_opt = if font >= 0 { Some(font) } else { None };
    let audience_opt: Option<Vec<String>> = if !audience_json.is_null() {
        CStr::from_ptr(audience_json)
            .to_str()
            .ok()
            .and_then(|s| serde_json::from_str(s).ok())
    } else {
        None
    };

    match h.runtime.block_on(
        h.client
            .send_text_status(text_str, bg_str, font_opt, audience_opt),
    ) {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_status_image(
    handle: *mut BaileysClientHandle,
    image_bytes: *const u8,
    image_len: usize,
    caption: *const c_char,
    mimetype: *const c_char,
) -> *mut c_char {
    if handle.is_null() || image_bytes.is_null() || image_len == 0 {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let bytes = std::slice::from_raw_parts(image_bytes, image_len);
    let cap = if !caption.is_null() {
        CStr::from_ptr(caption).to_str().ok()
    } else {
        None
    };
    let mime = if !mimetype.is_null() {
        CStr::from_ptr(mimetype).to_str().ok()
    } else {
        None
    };

    match h
        .runtime
        .block_on(h.client.send_status_image(bytes, cap, mime, None))
    {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_status_video(
    handle: *mut BaileysClientHandle,
    video_bytes: *const u8,
    video_len: usize,
    caption: *const c_char,
    mimetype: *const c_char,
) -> *mut c_char {
    if handle.is_null() || video_bytes.is_null() || video_len == 0 {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let bytes = std::slice::from_raw_parts(video_bytes, video_len);
    let cap = if !caption.is_null() {
        CStr::from_ptr(caption).to_str().ok()
    } else {
        None
    };
    let mime = if !mimetype.is_null() {
        CStr::from_ptr(mimetype).to_str().ok()
    } else {
        None
    };

    match h
        .runtime
        .block_on(h.client.send_status_video(bytes, cap, mime, None))
    {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_status_read_receipt(
    handle: *mut BaileysClientHandle,
    msg_id: *const c_char,
    author_jid: *const c_char,
) -> c_int {
    if handle.is_null() || msg_id.is_null() || author_jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let mid = match CStr::from_ptr(msg_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let ajid = match CStr::from_ptr(author_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h
        .runtime
        .block_on(h.client.send_status_read_receipt(mid, ajid))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_recent_stories(
    handle: *mut BaileysClientHandle,
    limit: u32,
    unviewed_only: c_int,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let unviewed = unviewed_only != 0;
    match h
        .runtime
        .block_on(h.client.get_recent_stories(limit, unviewed))
    {
        Ok(stories) => {
            let json = serde_json::to_string(&stories).unwrap_or_else(|_| "[]".to_string());
            CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_stories_by_author(
    handle: *mut BaileysClientHandle,
    author_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || author_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let ajid = match CStr::from_ptr(author_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.get_stories_by_author(ajid)) {
        Ok(stories) => {
            let json = serde_json::to_string(&stories).unwrap_or_else(|_| "[]".to_string());
            CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_fetch_privacy_settings(
    handle: *mut BaileysClientHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    match h.runtime.block_on(h.client.fetch_privacy_settings()) {
        Ok(settings) => {
            let json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".to_string());
            CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_update_privacy_setting(
    handle: *mut BaileysClientHandle,
    category: *const c_char,
    value: *const c_char,
) -> c_int {
    if handle.is_null() || category.is_null() || value.is_null() {
        return -1;
    }
    let h = &*handle;
    let cat = match CStr::from_ptr(category).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let val = match CStr::from_ptr(value).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.update_privacy_setting(cat, val))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_fetch_blocklist(
    handle: *mut BaileysClientHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    match h.runtime.block_on(h.client.fetch_blocklist()) {
        Ok(list) => {
            let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string());
            CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_blocked_contacts(
    handle: *mut BaileysClientHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    match h.runtime.block_on(h.client.get_blocked_contacts()) {
        Ok(list) => {
            let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string());
            CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_update_block_status(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    action: *const c_char,
) -> c_int {
    if handle.is_null() || jid.is_null() || action.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let act_str = match CStr::from_ptr(action).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.update_block_status(jid_str, act_str))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_update_profile_status(
    handle: *mut BaileysClientHandle,
    status: *const c_char,
) -> c_int {
    if handle.is_null() || status.is_null() {
        return -1;
    }
    let h = &*handle;
    let st = match CStr::from_ptr(status).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.update_profile_status(st)) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_fetch_status(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.fetch_status(jid_str)) {
        Ok(st) => {
            let json = serde_json::to_string(&st).unwrap_or_else(|_| "null".to_string());
            CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_business_profile(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.get_business_profile(jid_str)) {
        Ok(bp) => {
            let json = serde_json::to_string(&bp).unwrap_or_else(|_| "null".to_string());
            CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_edit_message(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    target_msg_id: *const c_char,
    new_text: *const c_char,
) -> *mut c_char {
    if handle.is_null() || chat_jid.is_null() || target_msg_id.is_null() || new_text.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let msg_id = match CStr::from_ptr(target_msg_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let text = match CStr::from_ptr(new_text).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h
        .runtime
        .block_on(h.client.edit_message(chat, msg_id, text))
    {
        Ok(id) => CString::new(id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_revoke_message(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    target_msg_id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || chat_jid.is_null() || target_msg_id.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let msg_id = match CStr::from_ptr(target_msg_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.revoke_message(chat, msg_id)) {
        Ok(id) => CString::new(id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_set_chat_ephemeral(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    expiration_seconds: u32,
) -> c_int {
    if handle.is_null() || chat_jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.set_chat_ephemeral(chat, expiration_seconds))
    {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_chat_ephemeral(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    out_expiration: *mut u32,
    out_timestamp: *mut i64,
) -> c_int {
    if handle.is_null() || chat_jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.get_chat_ephemeral(chat)) {
        Ok(Some((exp, ts))) => {
            if !out_expiration.is_null() {
                *out_expiration = exp;
            }
            if !out_timestamp.is_null() {
                *out_timestamp = ts.unwrap_or(0);
            }
            0
        }
        Ok(None) => 1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_labels(
    handle: *mut BaileysClientHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    match h.runtime.block_on(h.client.get_labels()) {
        Ok(list) => {
            if let Ok(json) = serde_json::to_string(&list) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_label(
    handle: *mut BaileysClientHandle,
    id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || id.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let label_id = match CStr::from_ptr(id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.get_label(label_id)) {
        Ok(Some(l)) => {
            if let Ok(json) = serde_json::to_string(&l) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Ok(None) => std::ptr::null_mut(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_upsert_label(
    handle: *mut BaileysClientHandle,
    id: *const c_char,
    name: *const c_char,
    color: c_int,
    predefined_id: c_int,
) -> *mut c_char {
    if handle.is_null() || name.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let id_opt = if id.is_null() {
        None
    } else {
        CStr::from_ptr(id).to_str().ok()
    };
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let predef_opt = if predefined_id > 0 {
        Some(predefined_id)
    } else {
        None
    };
    match h
        .runtime
        .block_on(h.client.upsert_label(id_opt, name_str, color, predef_opt))
    {
        Ok(assigned_id) => CString::new(assigned_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_delete_label(
    handle: *mut BaileysClientHandle,
    id: *const c_char,
) -> c_int {
    if handle.is_null() || id.is_null() {
        return -1;
    }
    let h = &*handle;
    let label_id = match CStr::from_ptr(id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.delete_label(label_id)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_add_chat_label(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    label_id: *const c_char,
) -> c_int {
    if handle.is_null() || chat_jid.is_null() || label_id.is_null() {
        return -1;
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let lid = match CStr::from_ptr(label_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.add_chat_label(chat, lid)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_remove_chat_label(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    label_id: *const c_char,
) -> c_int {
    if handle.is_null() || chat_jid.is_null() || label_id.is_null() {
        return -1;
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let lid = match CStr::from_ptr(label_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.remove_chat_label(chat, lid)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_chat_labels(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || chat_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.get_chat_labels(chat)) {
        Ok(list) => {
            if let Ok(json) = serde_json::to_string(&list) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_chats_by_label(
    handle: *mut BaileysClientHandle,
    label_id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || label_id.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let lid = match CStr::from_ptr(label_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.get_chats_by_label(lid)) {
        Ok(list) => {
            if let Ok(json) = serde_json::to_string(&list) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_add_message_label(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    message_id: *const c_char,
    label_id: *const c_char,
) -> c_int {
    if handle.is_null() || chat_jid.is_null() || message_id.is_null() || label_id.is_null() {
        return -1;
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let msg_id = match CStr::from_ptr(message_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let lid = match CStr::from_ptr(label_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.add_message_label(chat, msg_id, lid))
    {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_remove_message_label(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    message_id: *const c_char,
    label_id: *const c_char,
) -> c_int {
    if handle.is_null() || chat_jid.is_null() || message_id.is_null() || label_id.is_null() {
        return -1;
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let msg_id = match CStr::from_ptr(message_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let lid = match CStr::from_ptr(label_id).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.remove_message_label(chat, msg_id, lid))
    {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_message_labels(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    message_id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || chat_jid.is_null() || message_id.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let chat = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let msg_id = match CStr::from_ptr(message_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h
        .runtime
        .block_on(h.client.get_message_labels(chat, msg_id))
    {
        Ok(list) => {
            if let Ok(json) = serde_json::to_string(&list) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_messages_by_label(
    handle: *mut BaileysClientHandle,
    label_id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || label_id.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let lid = match CStr::from_ptr(label_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.get_messages_by_label(lid)) {
        Ok(list) => {
            if let Ok(json) = serde_json::to_string(&list) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

// ==========================================
// Track 10: Rich Messages & Attachments
// ==========================================

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_location(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    latitude: f64,
    longitude: f64,
    name: *const c_char,
    address: *const c_char,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let name_opt = if !name.is_null() {
        CStr::from_ptr(name).to_str().ok()
    } else {
        None
    };
    let addr_opt = if !address.is_null() {
        CStr::from_ptr(address).to_str().ok()
    } else {
        None
    };

    match h.runtime.block_on(
        h.client
            .send_location(to, latitude, longitude, name_opt, addr_opt, None),
    ) {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_contact(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    display_name: *const c_char,
    phone_number: *const c_char,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || display_name.is_null() || phone_number.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let name = match CStr::from_ptr(display_name).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let phone = match CStr::from_ptr(phone_number).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.send_contact(to, name, phone)) {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_group_invite(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    group_jid: *const c_char,
    invite_code: *const c_char,
    invite_expiration: i64,
    group_name: *const c_char,
    caption: *const c_char,
) -> *mut c_char {
    if handle.is_null()
        || to_jid.is_null()
        || group_jid.is_null()
        || invite_code.is_null()
        || group_name.is_null()
    {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let gjid = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let code = match CStr::from_ptr(invite_code).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let name = match CStr::from_ptr(group_name).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let cap_opt = if !caption.is_null() {
        CStr::from_ptr(caption).to_str().ok()
    } else {
        None
    };

    match h.runtime.block_on(h.client.send_group_invite(
        to,
        gjid,
        code,
        invite_expiration,
        name,
        cap_opt,
    )) {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_group_fetch_metadata(
    handle: *mut BaileysClientHandle,
    group_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || group_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let gjid = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.group_fetch_metadata(gjid)) {
        Ok(meta) => match serde_json::to_string(&meta) {
            Ok(json) => CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_group_fetch_all_participating(
    handle: *mut BaileysClientHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;

    match h.runtime.block_on(h.client.group_fetch_all_participating()) {
        Ok(groups) => match serde_json::to_string(&groups) {
            Ok(json) => CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_group_join_approval_mode(
    handle: *mut BaileysClientHandle,
    group_jid: *const c_char,
    mode: *const c_char,
) -> c_int {
    if handle.is_null() || group_jid.is_null() || mode.is_null() {
        return -1;
    }
    let h = &*handle;
    let gjid = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let m = match CStr::from_ptr(mode).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h
        .runtime
        .block_on(h.client.group_join_approval_mode(gjid, m))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_group_request_participants_list(
    handle: *mut BaileysClientHandle,
    group_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || group_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let gjid = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h
        .runtime
        .block_on(h.client.group_request_participants_list(gjid))
    {
        Ok(reqs) => match serde_json::to_string(&reqs) {
            Ok(json) => CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_group_request_participants_update(
    handle: *mut BaileysClientHandle,
    group_jid: *const c_char,
    participants_json: *const c_char,
    action: *const c_char,
) -> *mut c_char {
    if handle.is_null() || group_jid.is_null() || participants_json.is_null() || action.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let gjid = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let act = match CStr::from_ptr(action).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let parts_str = match CStr::from_ptr(participants_json).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let participants: Vec<String> = match serde_json::from_str(parts_str) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(
        h.client
            .group_request_participants_update(gjid, &participants, act),
    ) {
        Ok(res) => match serde_json::to_string(&res) {
            Ok(json) => CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_group_member_add_mode(
    handle: *mut BaileysClientHandle,
    group_jid: *const c_char,
    mode: *const c_char,
) -> c_int {
    if handle.is_null() || group_jid.is_null() || mode.is_null() {
        return -1;
    }
    let h = &*handle;
    let gjid = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let m = match CStr::from_ptr(mode).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h.runtime.block_on(h.client.group_member_add_mode(gjid, m)) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_group_revoke_invite_v4(
    handle: *mut BaileysClientHandle,
    group_jid: *const c_char,
    invited_jid: *const c_char,
) -> c_int {
    if handle.is_null() || group_jid.is_null() || invited_jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let gjid = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let p_jid = match CStr::from_ptr(invited_jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match h
        .runtime
        .block_on(h.client.group_revoke_invite_v4(gjid, p_jid))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_group_accept_invite_v4(
    handle: *mut BaileysClientHandle,
    group_jid: *const c_char,
    invite_code: *const c_char,
    invite_expiration: i64,
    admin_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || group_jid.is_null() || invite_code.is_null() || admin_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let gjid = match CStr::from_ptr(group_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let code = match CStr::from_ptr(invite_code).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let admin = match CStr::from_ptr(admin_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(
        h.client
            .group_accept_invite_v4(gjid, code, invite_expiration, admin),
    ) {
        Ok(joined_jid) => CString::new(joined_jid)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_catalog(
    handle: *mut BaileysClientHandle,
    biz_jid: *const c_char,
    limit: u32,
    cursor: *const c_char,
) -> *mut c_char {
    if handle.is_null() || biz_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let bjid = match CStr::from_ptr(biz_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let cur = if !cursor.is_null() {
        CStr::from_ptr(cursor).to_str().ok()
    } else {
        None
    };

    match h.runtime.block_on(h.client.get_catalog(bjid, limit, cur)) {
        Ok(res) => {
            if let Ok(json) = serde_json::to_string(&res) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_product(
    handle: *mut BaileysClientHandle,
    biz_jid: *const c_char,
    product_id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || biz_jid.is_null() || product_id.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let bjid = match CStr::from_ptr(biz_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let pid = match CStr::from_ptr(product_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.get_product(bjid, pid)) {
        Ok(p) => {
            if let Ok(json) = serde_json::to_string(&p) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_collections(
    handle: *mut BaileysClientHandle,
    biz_jid: *const c_char,
    limit: u32,
    cursor: *const c_char,
) -> *mut c_char {
    if handle.is_null() || biz_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let bjid = match CStr::from_ptr(biz_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let cur = if !cursor.is_null() {
        CStr::from_ptr(cursor).to_str().ok()
    } else {
        None
    };

    match h
        .runtime
        .block_on(h.client.get_collections(bjid, limit, cur))
    {
        Ok(res) => {
            if let Ok(json) = serde_json::to_string(&res) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_product(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    biz_owner_jid: *const c_char,
    product_id: *const c_char,
    title: *const c_char,
    price_1000: i64,
    currency: *const c_char,
    caption: *const c_char,
) -> *mut c_char {
    if handle.is_null()
        || to_jid.is_null()
        || biz_owner_jid.is_null()
        || product_id.is_null()
        || title.is_null()
        || currency.is_null()
    {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let biz = match CStr::from_ptr(biz_owner_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let pid = match CStr::from_ptr(product_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let tit = match CStr::from_ptr(title).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let curr = match CStr::from_ptr(currency).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let cap = if !caption.is_null() {
        CStr::from_ptr(caption).to_str().ok()
    } else {
        None
    };

    let product = baileys_core::CatalogProduct {
        id: pid.to_string(),
        biz_jid: biz.to_string(),
        title: tit.to_string(),
        description: cap.map(|s| s.to_string()),
        currency: curr.to_string(),
        price_amount_1000: price_1000,
        sale_price_amount_1000: None,
        retailer_id: None,
        url: None,
        image_urls: Vec::new(),
        is_hidden: false,
        availability: "in_stock".to_string(),
    };

    match h
        .runtime
        .block_on(h.client.send_product_message(to, biz, &product, cap, None))
    {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_order(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    seller_jid: *const c_char,
    order_id: *const c_char,
    title: *const c_char,
    item_count: i32,
    total_1000: i64,
    currency: *const c_char,
    message: *const c_char,
) -> *mut c_char {
    if handle.is_null()
        || to_jid.is_null()
        || seller_jid.is_null()
        || order_id.is_null()
        || title.is_null()
        || currency.is_null()
    {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let seller = match CStr::from_ptr(seller_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let oid = match CStr::from_ptr(order_id).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let tit = match CStr::from_ptr(title).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let curr = match CStr::from_ptr(currency).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let msg = if !message.is_null() {
        CStr::from_ptr(message).to_str().ok()
    } else {
        None
    };

    match h.runtime.block_on(h.client.send_order_message(
        to, seller, oid, tit, item_count, total_1000, curr, msg, None,
    )) {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_cached_orders(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    limit: u32,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let chat = if !chat_jid.is_null() {
        CStr::from_ptr(chat_jid).to_str().unwrap_or("")
    } else {
        ""
    };

    match h.runtime.block_on(h.client.get_cached_orders(chat, limit)) {
        Ok(orders) => {
            if let Ok(json) = serde_json::to_string(&orders) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_cached_catalog(
    handle: *mut BaileysClientHandle,
    biz_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || biz_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let bjid = match CStr::from_ptr(biz_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.get_cached_catalog(bjid)) {
        Ok(prods) => {
            if let Ok(json) = serde_json::to_string(&prods) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_cached_collections(
    handle: *mut BaileysClientHandle,
    biz_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || biz_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let bjid = match CStr::from_ptr(biz_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match h.runtime.block_on(h.client.get_cached_collections(bjid)) {
        Ok(cols) => {
            if let Ok(json) = serde_json::to_string(&cols) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_user_devices(
    handle: *mut BaileysClientHandle,
    user_jid: *const c_char,
) -> *mut c_char {
    if handle.is_null() || user_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let jid = match CStr::from_ptr(user_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match h.runtime.block_on(h.client.get_user_devices(jid)) {
        Ok(devs) => {
            if let Ok(json) = serde_json::to_string(&devs) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_get_paired_devices(
    handle: *mut BaileysClientHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    match h.runtime.block_on(h.client.get_paired_devices()) {
        Ok(devs) => {
            if let Ok(json) = serde_json::to_string(&devs) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_remove_companion_device(
    handle: *mut BaileysClientHandle,
    device_id: u32,
) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let h = &*handle;
    match h
        .runtime
        .block_on(h.client.remove_companion_device(device_id))
    {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_on_whatsapp(
    handle: *mut BaileysClientHandle,
    numbers: *const c_char,
) -> *mut c_char {
    if handle.is_null() || numbers.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let raw = match CStr::from_ptr(numbers).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let list: Vec<String> = if raw.trim_start().starts_with('[') {
        serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
    } else {
        raw.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let refs: Vec<&str> = list.iter().map(|s| s.as_str()).collect();
    match h.runtime.block_on(h.client.check_on_whatsapp(&refs)) {
        Ok(results) => {
            if let Ok(json) = serde_json::to_string(&results) {
                CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_chat_clear(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    keep_starred: c_int,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.chat_modify_clear(jid_str, keep_starred != 0))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_chat_delete(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h.runtime.block_on(h.client.chat_modify_delete(jid_str)) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_chat_mark_unread(
    handle: *mut BaileysClientHandle,
    jid: *const c_char,
    unread: c_int,
) -> c_int {
    if handle.is_null() || jid.is_null() {
        return -1;
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(jid).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match h
        .runtime
        .block_on(h.client.chat_modify_mark_unread(jid_str, unread != 0))
    {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_fetch_message_history(
    handle: *mut BaileysClientHandle,
    chat_jid: *const c_char,
    count: u32,
    oldest_msg_id: *const c_char,
    oldest_from_me: c_int,
    oldest_timestamp: u64,
) -> *mut c_char {
    if handle.is_null() || chat_jid.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let jid_str = match CStr::from_ptr(chat_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let oldest_id_str = if oldest_msg_id.is_null() {
        ""
    } else {
        match CStr::from_ptr(oldest_msg_id).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        }
    };
    match h.runtime.block_on(h.client.fetch_message_history(
        jid_str,
        count,
        oldest_id_str,
        oldest_from_me != 0,
        oldest_timestamp,
    )) {
        Ok(iq_id) => CString::new(iq_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_voice_note(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    audio_data: *const u8,
    audio_len: usize,
    seconds: u32,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || audio_data.is_null() || audio_len == 0 {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to_str = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let bytes = std::slice::from_raw_parts(audio_data, audio_len);
    match h
        .runtime
        .block_on(h.client.send_voice_note(to_str, bytes, seconds, None))
    {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_document(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    doc_data: *const u8,
    doc_len: usize,
    filename: *const c_char,
    mimetype: *const c_char,
    caption: *const c_char,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || doc_data.is_null() || doc_len == 0 {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to_str = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let fname = if !filename.is_null() {
        CStr::from_ptr(filename).to_str().unwrap_or("document.bin")
    } else {
        "document.bin"
    };
    let mime = if !mimetype.is_null() {
        CStr::from_ptr(mimetype)
            .to_str()
            .unwrap_or("application/octet-stream")
    } else {
        "application/octet-stream"
    };
    let cap = if !caption.is_null() {
        CStr::from_ptr(caption).to_str().ok()
    } else {
        None
    };
    let bytes = std::slice::from_raw_parts(doc_data, doc_len);
    match h.runtime.block_on(
        h.client
            .send_document(to_str, bytes, fname, mime, None, None, cap),
    ) {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_video(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    video_data: *const u8,
    video_len: usize,
    caption: *const c_char,
    is_gif: c_int,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || video_data.is_null() || video_len == 0 {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to_str = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let cap = if !caption.is_null() {
        CStr::from_ptr(caption).to_str().ok()
    } else {
        None
    };
    let bytes = std::slice::from_raw_parts(video_data, video_len);
    match h
        .runtime
        .block_on(h.client.send_video(to_str, bytes, cap, is_gif != 0, None))
    {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_sticker(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    sticker_data: *const u8,
    sticker_len: usize,
    is_animated: c_int,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || sticker_data.is_null() || sticker_len == 0 {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to_str = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let bytes = std::slice::from_raw_parts(sticker_data, sticker_len);
    match h
        .runtime
        .block_on(h.client.send_sticker(to_str, bytes, is_animated != 0))
    {
        Ok(msg_id) => CString::new(msg_id)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_json(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    json_str: *const c_char,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || json_str.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let json_text = match CStr::from_ptr(json_str).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let json_val = match serde_json::from_str::<serde_json::Value>(json_text) {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };

    match h
        .runtime
        .block_on(h.client.send_message_json(to, &json_val))
    {
        Ok(msg_id) => match CString::new(msg_id) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_send_event(
    handle: *mut BaileysClientHandle,
    to_jid: *const c_char,
    name: *const c_char,
    description: *const c_char,
    start_time: i64,
    end_time: i64,
    location: *const c_char,
) -> *mut c_char {
    if handle.is_null() || to_jid.is_null() || name.is_null() {
        return std::ptr::null_mut();
    }
    let h = &*handle;
    let to = match CStr::from_ptr(to_jid).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let event_name = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let desc = if !description.is_null() {
        CStr::from_ptr(description).to_str().ok()
    } else {
        None
    };
    let end_opt = if end_time > 0 { Some(end_time) } else { None };
    let loc = if !location.is_null() {
        CStr::from_ptr(location).to_str().ok()
    } else {
        None
    };

    match h.runtime.block_on(
        h.client
            .send_event(to, event_name, desc, start_time, end_opt, loc),
    ) {
        Ok(msg_id) => match CString::new(msg_id) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_recognize_message_json(
    proto_bytes: *const u8,
    proto_len: usize,
) -> *mut c_char {
    if proto_bytes.is_null() || proto_len == 0 {
        return std::ptr::null_mut();
    }
    use baileys_core::MessageExt;
    use prost::Message as ProstMessage;

    let slice = std::slice::from_raw_parts(proto_bytes, proto_len);
    let msg = match baileys_core::baileys_proto::proto::Message::decode(slice) {
        Ok(m) => m,
        Err(_) => return std::ptr::null_mut(),
    };

    let unwrap_res = msg.unwrap_wrappers();
    let text = msg.extract_text();
    let is_media = unwrap_res.real_type.is_media();
    let is_interactive = unwrap_res.real_type.is_interactive();

    let res_json = serde_json::json!({
        "real_type": unwrap_res.real_type.as_str(),
        "display_name": unwrap_res.real_type.display_name(),
        "wrappers": unwrap_res.wrappers,
        "text": text,
        "is_media": is_media,
        "is_interactive": is_interactive,
    });

    match CString::new(res_json.to_string()) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn baileys_client_destroy(handle: *mut BaileysClientHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cabi_lifecycle() {
        let path = CString::new(":memory:").unwrap();
        let ws_url = CString::new("mock://cabi").unwrap();
        unsafe {
            let handle = baileys_client_create_with_url(path.as_ptr(), ws_url.as_ptr());
            assert!(!handle.is_null());

            let res = baileys_client_connect(handle);
            assert_eq!(res, 0);

            let to = CString::new("123456789@s.whatsapp.net").unwrap();
            let msg = CString::new("Test message from C-ABI").unwrap();
            let msg_id = baileys_client_send_text(handle, to.as_ptr(), msg.as_ptr());
            assert!(!msg_id.is_null());

            baileys_free_string(msg_id);
            baileys_client_disconnect(handle);
            baileys_client_destroy(handle);
        }
    }

    #[test]
    fn test_cabi_chat_actions_and_rich_media() {
        let path = CString::new(":memory:").unwrap();
        let ws_url = CString::new("mock://cabi").unwrap();
        unsafe {
            let handle = baileys_client_create_with_url(path.as_ptr(), ws_url.as_ptr());
            assert!(!handle.is_null());

            let jid = CString::new("123456789@s.whatsapp.net").unwrap();

            // Test chat actions
            let res_clear = baileys_client_chat_clear(handle, jid.as_ptr(), 0);
            assert_eq!(res_clear, 0);

            let res_unread = baileys_client_chat_mark_unread(handle, jid.as_ptr(), 1);
            assert_eq!(res_unread, 0);

            let res_del = baileys_client_chat_delete(handle, jid.as_ptr());
            assert_eq!(res_del, 0);

            // Test null safety
            assert_eq!(
                baileys_client_chat_clear(std::ptr::null_mut(), jid.as_ptr(), 0),
                -1
            );
            assert_eq!(baileys_client_chat_delete(handle, std::ptr::null()), -1);
            assert_eq!(
                baileys_client_chat_mark_unread(handle, std::ptr::null(), 0),
                -1
            );
            assert!(
                baileys_client_send_voice_note(handle, jid.as_ptr(), std::ptr::null(), 0, 5)
                    .is_null()
            );
            assert!(baileys_client_send_document(
                handle,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null()
            )
            .is_null());
            assert!(baileys_client_send_video(
                handle,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                0
            )
            .is_null());
            assert!(
                baileys_client_send_sticker(handle, std::ptr::null(), std::ptr::null(), 0, 0)
                    .is_null()
            );

            baileys_client_destroy(handle);
        }
    }

    #[test]
    fn test_cabi_recognize_message_json() {
        use prost::Message as ProstMessage;
        let msg = baileys_core::baileys_proto::proto::Message {
            conversation: Some("Hello from C-ABI!".to_string()),
            ..Default::default()
        };
        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();

        unsafe {
            let json_ptr = baileys_client_recognize_message_json(buf.as_ptr(), buf.len());
            assert!(!json_ptr.is_null());
            let c_str = CStr::from_ptr(json_ptr);
            let s = c_str.to_str().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(s).unwrap();
            assert_eq!(parsed["real_type"], "conversation");
            assert_eq!(parsed["text"], "Hello from C-ABI!");
            baileys_free_string(json_ptr);
        }
    }
}
