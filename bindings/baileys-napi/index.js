const path = require('path');
const EventEmitter = require('events');
const { mapRawEvent } = require('./event-map');
const { createSockApi } = require('./sock-api');

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
  const dbPath = config.databasePath || config.authPath || 'rolleys_session.db';
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
        case 'raw':
          for (const [name, payload] of mapRawEvent(data)) {
            ev.emit(name, payload);
          }
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

  return createSockApi(client, ev);
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

// Same as Baileys' Browsers (Utils/browser-utils.ts): [platform, browser, version].
const PLATFORM_MAP = { aix: 'AIX', darwin: 'Mac OS', win32: 'Windows', android: 'Android', freebsd: 'FreeBSD', openbsd: 'OpenBSD', sunos: 'Solaris', linux: undefined, haiku: undefined, cygwin: undefined, netbsd: undefined };
const Browsers = {
  ubuntu: (browser) => ['Ubuntu', browser, '22.04.4'],
  macOS: (browser) => ['Mac OS', browser, '14.4.1'],
  baileys: (browser) => ['Baileys', browser, '6.5.0'],
  windows: (browser) => ['Windows', browser, '10.0.22631'],
  android: (browser) => [browser, 'Android', ''],
  appropriate: (browser) => [PLATFORM_MAP[require('os').platform()] || 'Ubuntu', browser, require('os').release()],
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

