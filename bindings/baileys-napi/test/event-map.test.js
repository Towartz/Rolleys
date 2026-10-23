'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { mapRawEvent } = require('../event-map');

// Fixtures use the serde form of `BaileysEvent` (pinned on the Rust side by
// `raw_event_serde_shape` in src/lib.rs).

test('creds.update from the unit variant', () => {
  assert.deepEqual(mapRawEvent('CredsUpdate'), [['creds.update', {}]]);
});

test('groups.update carries only changed fields', () => {
  const [[name, [u]]] = mapRawEvent({
    GroupsUpdate: {
      update: {
        id: '1@g.us', author: '2@s.whatsapp.net', subject: 'New', subject_owner: null,
        subject_time: 1700, description: null, description_deleted: false, description_id: null,
        announce: true, restrict: null, join_approval_mode: null, member_add_mode: null,
        ephemeral_duration: null, invite_code: null,
      },
    },
  });
  assert.equal(name, 'groups.update');
  assert.deepEqual(u, { id: '1@g.us', author: '2@s.whatsapp.net', subject: 'New', subjectTime: 1700, announce: true });
});

test('group-participants.update uses Baileys participant shape', () => {
  const [[name, p]] = mapRawEvent({
    GroupParticipantsUpdate: {
      group_jid: '1@g.us', author: null, action: 'add',
      participants: [{ jid: '3@lid', phone_number: '3@s.whatsapp.net', lid: null, admin: null }],
    },
  });
  assert.equal(name, 'group-participants.update');
  assert.deepEqual(p, {
    id: '1@g.us', action: 'add',
    participants: [{ id: '3@lid', phoneNumber: '3@s.whatsapp.net' }],
  });
});

test('messages.reaction keys target and sender like Baileys', () => {
  const [[name, [r]]] = mapRawEvent({
    MessagesReaction: {
      reaction: {
        chat_jid: '1@g.us', message_id: 'T', target_from_me: true, target_participant: null,
        sender: '9@lid', sender_from_me: false, text: '', removed: true, sender_timestamp_ms: 42,
      },
    },
  });
  assert.equal(name, 'messages.reaction');
  assert.deepEqual(r.key, { remoteJid: '1@g.us', id: 'T', fromMe: true });
  assert.deepEqual(r.reaction, {
    text: '', key: { remoteJid: '1@g.us', fromMe: false, participant: '9@lid' }, senderTimestampMs: 42,
  });
});

test('settings.update turns boolean settings into booleans', () => {
  assert.deepEqual(mapRawEvent({ SettingsUpdate: { setting: 'unarchiveChats', value: 'true' } }),
    [['settings.update', { setting: 'unarchiveChats', value: true }]]);
  assert.deepEqual(mapRawEvent({ SettingsUpdate: { setting: 'locale', value: 'id_ID' } }),
    [['settings.update', { setting: 'locale', value: 'id_ID' }]]);
});

test('history sync becomes messaging-history.set and keeps the legacy progress event', () => {
  const events = mapRawEvent({
    HistorySync: {
      sync_type: 6, progress: 100,
      conversations: [{ id: '1@g.us', display_name: 'G', messages: [] }],
      status_v3_messages: [],
      contacts: [{ jid: '1@g.us', name: 'G', username: null, lid: null, phone_number: null, notify: null }],
      messages: [{ key: { remote_jid: '1@g.us', from_me: false, id: 'M', participant: null }, message_timestamp: 5 }],
      lid_pn_mappings: [{ lid: '9@lid', pn: '9@s.whatsapp.net' }],
      past_participants: [], is_latest: null, chunk_order: null, peer_data_request_session_id: 'S',
    },
  });
  assert.deepEqual(events.map(([n]) => n), ['messaging-history.set', 'sync:resume:progress']);
  const set = events[0][1];
  assert.equal(set.syncType, 6);
  assert.equal(set.chats[0].displayName, 'G');
  assert.deepEqual(set.messages[0].key, { remoteJid: '1@g.us', fromMe: false, id: 'M', participant: null });
  assert.deepEqual(set.contacts, [{ id: '1@g.us', name: 'G' }]);
  assert.equal(set.peerDataRequestSessionId, 'S');
  assert.equal('isLatest' in set, false, 'on-demand chunks have no isLatest');
  assert.deepEqual(events[1][1], { sync_type: 6, progress: 100, conversation_count: 1 });
});

// Core events the native addon already maps to its own typed payloads.
const NATIVE_TYPED = new Set([
  'AppStateSynced', 'BlocklistUpdate', 'Call', 'ChatEphemeralUpdate', 'ChatLabelUpdate', 'ChatUpdate',
  'Connected', 'Connecting', 'ContactUpdate', 'DeviceUpdate', 'Disconnected', 'IdentityChanged',
  'LabelEdit', 'MediaProgress', 'MediaUpdated', 'Message', 'MessageEdited', 'MessageLabelUpdate',
  'MessageRevoked', 'PairingCode', 'PollVote', 'Presence', 'QrCode', 'Receipt', 'StoryReceived',
]);

test('every core event is either typed natively or mapped to a Baileys event', () => {
  const src = fs.readFileSync(path.join(__dirname, '../../../crates/baileys-core/src/events.rs'), 'utf8');
  const body = src.slice(src.indexOf('pub enum BaileysEvent {'), src.indexOf('\n}\n', src.indexOf('pub enum BaileysEvent {')));
  const variants = [...body.matchAll(/^    ([A-Z][A-Za-z]+)\b/gm)].map((m) => m[1]);
  assert.ok(variants.length > 40, `parsed only ${variants.length} variants`);

  const unmapped = variants.filter((v) => {
    if (NATIVE_TYPED.has(v)) return false;
    // A minimal serde-shaped value is enough to see which branch handles the variant.
    const probe = v === 'CredsUpdate' ? v : { [v]: new Proxy({}, { get: () => [] }) };
    let mapped;
    try {
      mapped = mapRawEvent(probe);
    } catch {
      return false; // reached a real branch, which then needed actual fields
    }
    return mapped.length === 1 && mapped[0][0] === 'raw';
  });
  assert.deepEqual(unmapped, [], 'core events with no Baileys mapping');

  const stale = [...NATIVE_TYPED].filter((v) => !variants.includes(v));
  assert.deepEqual(stale, [], 'NATIVE_TYPED lists events core no longer has');
});
