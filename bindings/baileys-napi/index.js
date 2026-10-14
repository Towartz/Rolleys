const path = require('path');
const EventEmitter = require('events');

// Locate native binary
let nativeBinding = null;
const candidates = [
  path.join(__dirname, 'baileys_napi.node'),
  path.join(__dirname, 'baileys.win32-x64-msvc.node'),
  path.join(__dirname, 'baileys.node'),
  path.join(__dirname, '../../target/release/baileys_napi.dll'),
  path.join(__dirname, '../../target/release/baileys_napi.node'),
  path.join(__dirname, '../../target/debug/baileys_napi.dll'),
  path.join(__dirname, '../../target/debug/baileys_napi.node'),
];

for (const p of candidates) {
  try {
    nativeBinding = require(p);
    break;
  } catch (e) {
    // try next candidate
  }
}

if (!nativeBinding) {
  throw new Error('Failed to load Rose-Baileys native addon (baileys_napi.node / baileys_napi.dll)');
}

const { RoseBaileysClient } = nativeBinding;

/**
 * Creates a Baileys-compatible WhatsApp Socket engine backed by pure Rust.
 */
function makeWASocket(config = {}) {
  const dbPath = config.databasePath || config.authPath || 'aurora_session.db';
  const phone = config.phoneNumber || config.phone || null;

  const client = new RoseBaileysClient(dbPath, phone);
  const ev = new EventEmitter();

  // Register native event bridge
  client.onEvent((eventJson) => {
    try {
      const parsed = JSON.parse(eventJson);
      const { event, data } = parsed;

      // Map to standard Baileys event names
      switch (event) {
        case 'connecting':
          ev.emit('connection.update', { connection: 'connecting' });
          break;
        case 'qr':
          ev.emit('connection.update', { qr: data.code });
          ev.emit('auth:qr', data.code);
          break;
        case 'pairing_code':
          ev.emit('connection.update', { pairingCode: data.code });
          ev.emit('auth:pairing-code', { code: data.code, phone });
          break;
        case 'connected':
          ev.emit('connection.update', { connection: 'open', user: { id: data.me } });
          ev.emit('auth:open', { me: data.me });
          break;
        case 'message':
          ev.emit('messages.upsert', {
            type: 'notify',
            messages: [{
              key: {
                remoteJid: data.remote_jid,
                fromMe: data.from_me,
                id: data.id,
                participant: data.participant,
              },
              messageTimestamp: data.timestamp,
              summary: data.summary,
              hasMedia: data.has_media,
            }]
          });
          ev.emit('messages:new', data);
          break;
        case 'receipt':
          ev.emit('messages.update', [{
            key: { id: data.id, remoteJid: data.remote_jid },
            update: { status: data.status }
          }]);
          break;
        case 'presence':
          ev.emit('presence.update', { id: data.jid, presences: { [data.jid]: { lastKnownPresence: data.presence } } });
          break;
        case 'history_sync':
          ev.emit('sync:resume:progress', data);
          break;
        case 'message_edited':
          ev.emit('messages.update', [{
            key: { id: data.message_id, remoteJid: data.chat_jid },
            update: { message: { conversation: data.new_text } }
          }]);
          ev.emit('messages:edit', data);
          break;
        case 'message_revoked':
          ev.emit('messages.delete', {
            keys: [{ id: data.message_id, remoteJid: data.chat_jid, participant: data.participant }]
          });
          ev.emit('messages:revoke', data);
          break;
        case 'chat_ephemeral_update':
          ev.emit('chats.update', [{
            id: data.chat_jid,
            ephemeralExpiration: data.ephemeral_expiration
          }]);
          ev.emit('chats:ephemeral', data);
          break;
        case 'label_edit':
          ev.emit('labels.edit', data.label);
          ev.emit('label:edit', data);
          break;
        case 'chat_label_update':
          ev.emit('chats.label', { jid: data.chat_jid, labelId: data.label_id, labeled: data.labeled });
          ev.emit('chat:label', data);
          break;
        case 'message_label_update':
          ev.emit('messages.label', { jid: data.chat_jid, messageId: data.message_id, labelId: data.label_id, labeled: data.labeled });
          ev.emit('message:label', data);
          break;
        case 'device_update':
          ev.emit('user.devices', { userJid: data.user_jid, devices: data.devices });
          ev.emit('device:update', data);
          break;
        case 'disconnected':
          ev.emit('connection.update', { connection: 'close', reason: data.reason });
          break;
        default:
          ev.emit(event, data);
      }
    } catch (err) {
      console.error('[rose-baileys-napi] Error handling event:', err);
    }
  });

  return {
    ev,
    client,

    // Core Connection
    connect: () => client.connect(),
    disconnect: () => client.disconnect(),
    end: () => client.disconnect(),
    isConnected: () => client.isConnected(),
    getMe: () => client.meJid(),
    ping: () => client.ping(),

    // Messaging
    sendMessage: async (jid, content, options = {}) => {
      if (typeof content === 'string') {
        if (options.quoted && options.quoted.key) {
          return client.sendTextMessageWithQuote(
            jid,
            content,
            options.quoted.key.id,
            options.quoted.key.participant || null,
            options.quoted.summary || null
          );
        }
        return client.sendTextMessage(jid, content);
      } else if (content.text) {
        if (options.quoted && options.quoted.key) {
          return client.sendTextMessageWithQuote(
            jid,
            content.text,
            options.quoted.key.id,
            options.quoted.key.participant || null,
            options.quoted.summary || null
          );
        }
        return client.sendTextMessage(jid, content.text);
      } else if (content.image) {
        return client.sendMedia(jid, 'image', Buffer.from(content.image), content.caption || null, null, null, null, null, null);
      } else if (content.video) {
        return client.sendMedia(jid, 'video', Buffer.from(content.video), content.caption || null, null, null, null, content.seconds || null, content.gifPlayback || null);
      } else if (content.audio) {
        return client.sendMedia(jid, 'audio', Buffer.from(content.audio), null, null, null, content.ptt || false, content.seconds || null, null);
      } else if (content.document) {
        return client.sendMedia(jid, 'document', Buffer.from(content.document), content.caption || null, content.fileName || 'file.bin', content.mimetype || 'application/octet-stream', null, null, null);
      } else if (content.sticker) {
        return client.sendMedia(jid, 'sticker', Buffer.from(content.sticker), null, null, null, null, null, content.isAnimated || false);
      } else if (content.location) {
        return client.sendLocation(
          jid,
          content.location.degreesLatitude,
          content.location.degreesLongitude,
          content.location.name || null,
          content.location.address || null,
          content.location.url || null
        );
      } else if (content.liveLocation) {
        return client.sendLiveLocation(
          jid,
          content.liveLocation.degreesLatitude,
          content.liveLocation.degreesLongitude,
          content.liveLocation.accuracyInMeters || null,
          content.liveLocation.caption || null
        );
      } else if (content.contact) {
        return client.sendContact(
          jid,
          content.contact.displayName,
          content.contact.phoneNumber
        );
      } else if (content.groupInvite) {
        return client.sendGroupInvite(
          jid,
          content.groupInvite.groupJid,
          content.groupInvite.inviteCode,
          content.groupInvite.inviteExpiration,
          content.groupInvite.groupName,
          content.groupInvite.caption || null
        );
      } else if (content.product) {
        return client.sendProductMessage(
          jid,
          content.product.businessOwnerJid || jid,
          content.product.productId || content.product.id,
          content.product.title,
          content.product.priceAmount1000 || content.product.price1000 || 0,
          content.product.currency || 'USD',
          content.product.caption || content.caption || null
        );
      } else if (content.order) {
        return client.sendOrderMessage(
          jid,
          content.order.sellerJid || jid,
          content.order.orderId || content.order.id,
          content.order.title || 'Order',
          content.order.itemCount || 1,
          content.order.totalAmount1000 || content.order.total1000 || 0,
          content.order.currency || 'USD',
          content.order.message || null
        );
      }
      throw new Error('Unsupported message content format');
    },

    sendReaction: (jid, msgId, emoji) => client.sendReaction(jid, msgId, emoji),
    sendReadReceipt: (jid, msgId, participant) => client.sendReadReceipt(jid, msgId, participant),
    sendPresenceUpdate: (presence, toJid) => client.sendPresenceUpdate(presence, toJid),
    downloadMedia: (msgId, destDir) => client.downloadMediaForMessage(msgId, destDir),
    setAutoDownload: (enabled) => client.setAutoDownload(enabled),

    // Data Storage & History
    getChats: async () => JSON.parse(await client.getChats()),
    getMessages: async (chatJid, limit = 50, beforeTimestamp = null) => JSON.parse(await client.getMessages(chatJid, limit, beforeTimestamp)),
    getContact: async (jid) => {
      const res = await client.getContact(jid);
      return res ? JSON.parse(res) : null;
    },

    // Group Management
    groupMetadata: async (jid) => JSON.parse(await client.groupMetadata(jid)),
    groupCreate: (subject, participants) => client.groupCreate(subject, participants),
    groupParticipantsUpdate: async (jid, participants, action) => JSON.parse(await client.groupParticipantsUpdate(jid, participants, action)),
    groupUpdateSubject: (jid, subject) => client.groupUpdateSubject(jid, subject),
    groupUpdateDescription: (jid, desc) => client.groupUpdateDescription(jid, desc),
    groupInviteCode: (jid) => client.groupGetInviteCode(jid),
    groupAcceptInvite: (code) => client.groupAcceptInvite(code),
    groupLeave: (jid) => client.groupLeave(jid),
    groupSettingUpdate: (jid, setting) => client.groupSettingUpdate(jid, setting),
    groupRevokeInvite: (jid) => client.groupRevokeInvite(jid),
    groupFetchMetadata: (jid) => client.groupFetchMetadata(jid),
    groupFetchAllParticipating: () => client.groupFetchAllParticipating(),
    groupJoinApprovalMode: (jid, mode) => client.groupJoinApprovalMode(jid, mode),
    groupRequestParticipantsList: (jid) => client.groupRequestParticipantsList(jid),
    groupRequestParticipantsUpdate: (jid, participants, action) => client.groupRequestParticipantsUpdate(jid, participants, action),
    groupMemberAddMode: (jid, mode) => client.groupMemberAddMode(jid, mode),
    groupRevokeInviteV4: (groupJid, invitedJid) => client.groupRevokeInviteV4(groupJid, invitedJid),
    groupAcceptInviteV4: (groupJid, inviteCode, inviteExpiration, adminJid) => client.groupAcceptInviteV4(groupJid, inviteCode, inviteExpiration, adminJid),

    // Privacy & Profile
    profilePictureUrl: (jid, type) => client.profilePictureUrl(jid, type || null),
    updateProfilePicture: (jid, imageBuffer) => client.updateProfilePicture(jid || null, Buffer.from(imageBuffer)),
    removeProfilePicture: (jid) => client.removeProfilePicture(jid || null),
    presenceSubscribe: (jid) => client.presenceSubscribe(jid),
    updateProfileStatus: (status) => client.updateProfileStatus(status),
    fetchPrivacySettings: () => client.fetchPrivacySettings(),
    updatePrivacySetting: (category, value) => client.updatePrivacySetting(category, value),
    fetchBlocklist: () => client.fetchBlocklist(),
    updateBlockStatus: (jid, action) => client.updateBlockStatus(jid, action),

    // Message Editing, Revoking & Ephemeral
    editMessage: (jid, msgId, newText) => client.editMessage(jid, msgId, newText),
    revokeMessage: (jid, msgId) => client.revokeMessage(jid, msgId),
    setChatEphemeral: (jid, ephemeralExpiration) => client.setChatEphemeral(jid, ephemeralExpiration),
    getChatEphemeral: (jid) => client.getChatEphemeral(jid),

    // WhatsApp Business Labels
    getLabels: () => client.getLabels(),
    getLabel: (id) => client.getLabel(id),
    upsertLabel: (name, color = 0, id = null, predefinedId = null) => client.upsertLabel(id, name, color, predefinedId),
    deleteLabel: (id) => client.deleteLabel(id),
    addChatLabel: (jid, labelId) => client.addChatLabel(jid, labelId),
    removeChatLabel: (jid, labelId) => client.removeChatLabel(jid, labelId),
    getChatLabels: (jid) => client.getChatLabels(jid),
    getChatsByLabel: (labelId) => client.getChatsByLabel(labelId),
    addMessageLabel: (jid, msgId, labelId) => client.addMessageLabel(jid, msgId, labelId),
    removeMessageLabel: (jid, msgId, labelId) => client.removeMessageLabel(jid, msgId, labelId),
    getMessageLabels: (jid, msgId) => client.getMessageLabels(jid, msgId),
    getMessagesByLabel: (labelId) => client.getMessagesByLabel(labelId),

    // Rich Messages & Attachments
    sendLocation: (jid, lat, lng, name = null, address = null, url = null) =>
      client.sendLocation(jid, lat, lng, name, address, url),
    sendLiveLocation: (jid, lat, lng, accuracy = null, caption = null) =>
      client.sendLiveLocation(jid, lat, lng, accuracy, caption),
    sendContact: (jid, displayName, phoneNumber) =>
      client.sendContact(jid, displayName, phoneNumber),
    sendVCard: (jid, displayName, vcard) =>
      client.sendVcard(jid, displayName, vcard),
    sendGroupInvite: (jid, groupJid, inviteCode, inviteExpiration, groupName, caption = null) =>
      client.sendGroupInvite(jid, groupJid, inviteCode, inviteExpiration, groupName, caption),

    // Business Catalog, Products & Orders
    getCatalog: (bizJid, limit = 10, cursor = null) => client.getCatalog(bizJid, limit, cursor),
    getProduct: (bizJid, productId) => client.getProduct(bizJid, productId),
    getCollections: (bizJid, limit = 10, cursor = null) => client.getCollections(bizJid, limit, cursor),
    sendProductMessage: (toJid, bizOwnerJid, productId, title, priceAmount1000, currency = 'USD', caption = null) =>
      client.sendProductMessage(toJid, bizOwnerJid, productId, title, priceAmount1000, currency, caption),
    sendOrderMessage: (toJid, sellerJid, orderId, title, itemCount, totalAmount1000, currency = 'USD', message = null) =>
      client.sendOrderMessage(toJid, sellerJid, orderId, title, itemCount, totalAmount1000, currency, message),
    getCachedOrders: (chatJid = '', limit = 20) => client.getCachedOrders(chatJid, limit),
    getCachedCatalog: (bizJid) => client.getCachedCatalog(bizJid),
    getCachedCollections: (bizJid) => client.getCachedCollections(bizJid),

    // Multi-Device Companions & USync
    getUserDevices: (userJid) => client.getUserDevices(userJid),
    getPairedDevices: () => client.getPairedDevices(),
    removeCompanionDevice: (deviceId) => client.removeCompanionDevice(deviceId),
    onWhatsApp: (...numbers) => {
      const flat = numbers.flat();
      return client.onWhatsapp(flat);
    },

    // Chat Actions & Syncd (Clear, Delete, Unread, History Fetch)
    chatModify: async (mod, jid) => {
      if (mod.mute !== undefined) {
        return client.chatModifyMute(jid, mod.mute ? Math.floor(mod.mute / 1000) : 0);
      }
      if (mod.pin !== undefined) {
        return client.chatModifyPin(jid, !!mod.pin);
      }
      if (mod.archive !== undefined) {
        return client.chatModifyArchive(jid, !!mod.archive);
      }
      if (mod.star !== undefined) {
        const star = !!mod.star.star;
        for (const msg of (mod.star.messages || [])) {
          await client.chatModifyStar(jid, msg.id, msg.fromMe || false, star);
        }
        return;
      }
      if (mod.clear !== undefined) {
        const keepStarred = typeof mod.clear === 'object' ? !!mod.clear.keepStarred : false;
        return client.chatModifyClear(jid, keepStarred);
      }
      if (mod.delete) {
        return client.chatModifyDelete(jid);
      }
      if (mod.markRead !== undefined) {
        return client.chatModifyMarkUnread(jid, !mod.markRead);
      }
      if (mod.unread !== undefined) {
        return client.chatModifyMarkUnread(jid, !!mod.unread);
      }
    },
    chatModifyClear: (jid, keepStarred = false) => client.chatModifyClear(jid, keepStarred),
    chatModifyDelete: (jid) => client.chatModifyDelete(jid),
    chatModifyUnread: (jid, unread = true) => client.chatModifyMarkUnread(jid, unread),
    chatModifyPin: (jid, pin = true) => client.chatModifyPin(jid, pin),
    chatModifyMute: (jid, durationSecs = null) => client.chatModifyMute(jid, durationSecs),
    chatModifyArchive: (jid, archive = true) => client.chatModifyArchive(jid, archive),
    chatModifyStar: (jid, msgId, fromMe = false, star = true) => client.chatModifyStar(jid, msgId, fromMe, star),
    fetchMessageHistory: (jid, count = 50, oldestMsgId = null, oldestFromMe = false, oldestTimestamp = null) =>
      client.fetchMessageHistory(jid, count, oldestMsgId, oldestFromMe, oldestTimestamp),

    // Rich Media & Attachments Protocol
    sendVoiceNote: (jid, audioBuffer, seconds = 0, waveform = null) =>
      client.sendVoiceNote(jid, Buffer.from(audioBuffer), seconds, waveform ? Buffer.from(waveform) : null),
    sendDocument: (jid, docBuffer, fileName, mimetype = 'application/octet-stream', caption = null, pageCount = null) =>
      client.sendDocument(jid, Buffer.from(docBuffer), fileName, mimetype, caption, pageCount),
    sendVideo: (jid, videoBuffer, caption = null, isGif = false, seconds = null) =>
      client.sendVideo(jid, Buffer.from(videoBuffer), caption, isGif, seconds),
    sendSticker: (jid, stickerBuffer, isAnimated = false) =>
      client.sendSticker(jid, Buffer.from(stickerBuffer), isAnimated),
  };
}

function jidNormalizedUser(jid) {
  if (!jid) return '';
  const [userPart, server] = jid.split('@');
  const user = userPart.split(':')[0];
  if (!server) return user;
  return `${user}@${server}`;
}

function isJidGroup(jid) {
  return typeof jid === 'string' && jid.endsWith('@g.us');
}

function isJidBroadcast(jid) {
  return typeof jid === 'string' && jid.endsWith('@broadcast');
}

function isJidStatusBroadcast(jid) {
  return jid === 'status@broadcast';
}

function isLidUser(jid) {
  return typeof jid === 'string' && jid.endsWith('@lid');
}

function jidDecode(jid) {
  if (!jid) return undefined;
  const [userPart, server] = jid.split('@');
  if (!server) return undefined;
  const [user, device] = userPart.split(':');
  return {
    user,
    server,
    device: device ? parseInt(device, 10) : undefined,
    domainType: server === 'lid' ? 1 : 0
  };
}

const DisconnectReason = {
  connectionClosed: 428,
  connectionLost: 408,
  connectionReplaced: 440,
  timedOut: 408,
  loggedOut: 401,
  badSession: 500,
  restartRequired: 515,
  multicastTimeout: 504
};

const Browsers = {
  ubuntu: (browser) => ['Ubuntu', browser || 'Chrome', '22.04.4'],
  macOS: (browser) => ['macOS', browser || 'Chrome', '14.4.1'],
  windows: (browser) => ['Windows', browser || 'Chrome', '10.0.22631'],
  appropriate: (browser) => ['Rose-Baileys', browser || 'Desktop', '1.0.0'],
};

const WAMessageStatus = {
  ERROR: 0,
  PENDING: 1,
  SERVER_ACK: 2,
  DELIVERY_ACK: 3,
  READ: 4,
  PLAYED: 5,
};

module.exports = {
  makeWASocket,
  default: makeWASocket,
  RoseBaileysClient,
  jidNormalizedUser,
  isJidGroup,
  isJidBroadcast,
  isJidStatusBroadcast,
  isLidUser,
  jidDecode,
  DisconnectReason,
  Browsers,
  WAMessageStatus,
};

