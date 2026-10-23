'use strict';

/**
 * Translates core events (the serde form of `BaileysEvent`, forwarded by the native addon as
 * `{ event: 'raw', data }`) into Baileys event names and payload shapes.
 *
 * Pure: no native code, so it can be tested with `node --test`.
 */

const snakeToCamel = (key) => key.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

/** Recursively camelCases object keys (protobuf messages arrive with snake_case fields). */
function camelize(value) {
  if (Array.isArray(value)) return value.map(camelize);
  if (value && typeof value === 'object') {
    const out = {};
    for (const [k, v] of Object.entries(value)) out[snakeToCamel(k)] = camelize(v);
    return out;
  }
  return value;
}

/** Drops undefined/null fields so partial updates only carry what changed. */
function compact(obj) {
  const out = {};
  for (const [k, v] of Object.entries(obj)) {
    if (v !== undefined && v !== null) out[k] = v;
  }
  return out;
}

const participant = (p) =>
  compact({ id: p.jid, phoneNumber: p.phone_number, lid: p.lid, admin: p.admin || null });

const groupMetadata = (g) => ({
  id: g.id,
  subject: g.subject,
  subjectOwner: g.subject_owner,
  subjectTime: g.subject_time,
  size: g.size,
  creation: g.creation,
  owner: g.owner,
  desc: g.description,
  descId: g.description_id,
  restrict: g.restrict,
  announce: g.announce,
  isCommunity: g.is_community,
  isCommunityAnnounce: g.is_community_announce,
  joinApprovalMode: g.join_approval_mode,
  // Baileys exposes memberAddMode as "everyone may add members".
  memberAddMode: g.member_add_mode === 'all_member_add',
  ephemeralDuration: g.ephemeral_duration,
  addressingMode: g.addressing_mode,
  participants: (g.participants || []).map(participant),
});

const BOOLEAN_SETTINGS = new Set([
  'unarchiveChats',
  'disableLinkPreviews',
  'privacySettingRelayAllCalls',
  'channelsPersonalisedRecommendation',
]);

/**
 * @param {object} raw serde form of a core `BaileysEvent`, e.g. `{ "ChatsLock": { jid, locked } }`
 * @returns {Array<[string, any]>} Baileys events to emit, in order
 */
function mapRawEvent(raw) {
  if (raw === 'CredsUpdate') return [['creds.update', {}]];
  const [name] = Object.keys(raw || {});
  const d = raw ? raw[name] : undefined;
  switch (name) {
    case 'GroupsUpsert':
      return [['groups.upsert', d.groups.map(groupMetadata)]];
    case 'GroupsUpdate': {
      const u = d.update;
      return [[
        'groups.update',
        [compact({
          id: u.id,
          author: u.author,
          subject: u.subject,
          subjectOwner: u.subject_owner,
          subjectTime: u.subject_time,
          // A deleted description is reported as an empty one.
          desc: u.description_deleted ? '' : u.description,
          descId: u.description_id,
          announce: u.announce,
          restrict: u.restrict,
          joinApprovalMode: u.join_approval_mode,
          memberAddMode: u.member_add_mode == null ? undefined : u.member_add_mode === 'all_member_add',
          ephemeralDuration: u.ephemeral_duration,
          inviteCode: u.invite_code,
        })],
      ]];
    }
    case 'GroupParticipantsUpdate':
      return [['group-participants.update', compact({
        id: d.group_jid,
        author: d.author,
        participants: d.participants.map(participant),
        action: d.action,
      })]];
    case 'GroupJoinRequest':
      return [['group.join-request', compact({
        id: d.group_jid,
        author: d.author,
        participant: d.participant,
        participantPn: d.participant_pn,
        action: d.action,
        method: d.method,
      })]];
    case 'GroupMemberTagUpdate':
      return [['group.member-tag.update', compact({
        groupId: d.group_jid,
        participant: d.participant,
        participantAlt: d.participant_alt,
        label: d.label,
        messageTimestamp: d.message_timestamp,
      })]];
    case 'NewsletterReaction':
      return [['newsletter.reaction', {
        id: d.newsletter_jid,
        server_id: d.server_id,
        reaction: compact({ code: d.code, count: d.count, removed: d.removed }),
      }]];
    case 'NewsletterView':
      return [['newsletter.view', { id: d.newsletter_jid, server_id: d.server_id, count: d.count }]];
    case 'NewsletterParticipantsUpdate':
      return [['newsletter-participants.update', compact({
        id: d.newsletter_jid,
        author: d.author,
        user: d.user,
        new_role: d.new_role,
        action: d.action,
      })]];
    case 'NewsletterSettingsUpdate':
      return [['newsletter-settings.update', {
        id: d.newsletter_jid,
        update: compact({ name: d.name, description: d.description }),
      }]];
    case 'ChatsUpsert':
      return [['chats.upsert', d.chats.map((c) => compact({
        id: c.jid,
        name: c.name,
        conversationTimestamp: c.conversation_timestamp,
      }))]];
    case 'ChatsDelete':
      return [['chats.delete', d.jids]];
    case 'ChatsLock':
      return [['chats.lock', { id: d.jid, locked: d.locked }]];
    case 'SettingsUpdate':
      return [['settings.update', {
        setting: d.setting,
        value: BOOLEAN_SETTINGS.has(d.setting) ? d.value === 'true' : d.value,
      }]];
    case 'LidMappingUpdate':
      return [['lid-mapping.update', { lid: d.lid, pn: d.pn }]];
    case 'ContactsUpsert':
      return [['contacts.upsert', d.contacts.map((c) => compact({
        id: c.jid,
        name: c.name,
        notify: c.push_name,
        lid: c.lid,
      }))]];
    case 'MessagesDelete':
      return [['messages.delete', {
        keys: [{ remoteJid: d.chat_jid, id: d.message_id, fromMe: d.from_me }],
      }]];
    case 'MessageStarred':
      return [['messages.update', [{
        key: { remoteJid: d.chat_jid, id: d.message_id, fromMe: d.from_me },
        update: { starred: d.starred },
      }]]];
    case 'MessagesReaction': {
      const r = d.reaction;
      return [['messages.reaction', [{
        key: compact({
          remoteJid: r.chat_jid,
          id: r.message_id,
          fromMe: r.target_from_me,
          participant: r.target_participant,
        }),
        reaction: compact({
          // Empty text means the reaction was removed, as in Baileys.
          text: r.text,
          key: { remoteJid: r.chat_jid, fromMe: r.sender_from_me, participant: r.sender },
          senderTimestampMs: r.sender_timestamp_ms,
        }),
      }]]];
    }
    case 'MessagingHistoryStatus':
      return [['messaging-history.status', {
        syncType: d.sync_type,
        status: d.status,
        explicit: d.explicit,
      }]];
    case 'MessageCappingUpdate':
      // Baileys' NewChatMessageCapInfo keeps snake_case field names.
      return [['message-capping.update', compact(d.info)]];
    case 'HistorySync':
      return [
        ['messaging-history.set', compact({
          chats: camelize(d.conversations || []),
          contacts: (d.contacts || []).map((c) => compact({
            id: c.jid,
            name: c.name,
            notify: c.notify,
            username: c.username,
            lid: c.lid,
            phoneNumber: c.phone_number,
          })),
          messages: camelize(d.messages || []),
          lidPnMappings: d.lid_pn_mappings || [],
          pastParticipants: camelize(d.past_participants || []),
          syncType: d.sync_type,
          progress: d.progress,
          isLatest: d.is_latest,
          chunkOrder: d.chunk_order,
          peerDataRequestSessionId: d.peer_data_request_session_id,
        })],
        // Kept for code written against this addon before messaging-history.set existed.
        ['sync:resume:progress', {
          sync_type: d.sync_type,
          progress: d.progress,
          conversation_count: (d.conversations || []).length,
        }],
      ];
    default:
      return [['raw', raw]];
  }
}

module.exports = { mapRawEvent, camelize };
