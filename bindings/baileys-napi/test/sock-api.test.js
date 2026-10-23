'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const EventEmitter = require('node:events');
const { createSockApi } = require('../sock-api');

const { keys: BAILEYS_SOCK_KEYS } = require('./baileys-sock-keys.json');

/** Baileys socket keys deliberately absent from our socket, with the reason. */
const NOT_EXPOSED = {
  // Internal machinery of Baileys' JS implementation; the Rust core owns these concerns.
  appPatch: 'internal app-state patch builder',
  appStatePatchMutex: 'internal lock',
  assertSessions: 'internal Signal session setup',
  authState: 'credentials live in the Rust store',
  createParticipantNodes: 'internal stanza builder',
  devicesMutex: 'internal lock',
  digestKeyBundle: 'internal pre-key maintenance',
  executeUSyncQuery: 'internal query builder',
  generateMessageTag: 'internal id generator',
  getMediaHost: 'internal media routing',
  issuePrivacyTokens: 'internal tctoken handling',
  logger: 'logging is configured in Rust (RUST_LOG)',
  messageMutex: 'internal lock',
  messageRetryManager: 'internal retry state',
  notificationMutex: 'internal lock',
  onUnexpectedError: 'internal error hook',
  placeholderResendCache: 'internal cache',
  query: 'raw IQ access',
  receiptMutex: 'internal lock',
  refreshMediaConn: 'internal media host refresh',
  registerSocketEndHandler: 'internal lifecycle hook',
  relayMessage: 'takes a protobuf message; use sendMessage',
  rotateSignedPreKey: 'internal pre-key maintenance',
  sendMessageAck: 'internal ack',
  sendNode: 'raw binary node',
  sendPeerDataOperationMessage: 'takes a protobuf message',
  sendRawMessage: 'raw frame',
  sendReceipt: 'internal receipt; use readMessages',
  sendReceipts: 'internal receipt; use readMessages',
  sendRetryRequest: 'internal retry',
  sendUnifiedSession: 'internal telemetry',
  sendWAMBuffer: 'internal telemetry',
  serverProps: 'internal server config',
  signalRepository: 'Signal state lives in Rust',
  updateServerTimeOffset: 'internal clock sync',
  uploadPreKeys: 'internal pre-key maintenance',
  uploadPreKeysToServerIfRequired: 'internal pre-key maintenance',
  upsertMessage: 'internal message pipeline',
  userDevicesCache: 'internal cache',
  waUploadToServer: 'internal media upload',
  waitForMessage: 'internal IQ waiter',
  wamBuffer: 'internal telemetry',
  ws: 'the websocket lives in Rust',

  // Baileys features the Rust core does not implement yet.
  addOrEditContact: 'not implemented in core',
  removeContact: 'not implemented in core',
  addOrEditQuickReply: 'not implemented in core',
  removeQuickReply: 'not implemented in core',
  getOrderDetails: 'not implemented in core',
  updateBussinesProfile: 'not implemented in core',
  updateMemberLabel: 'not implemented in core',
  updateDefaultDisappearingMode: 'not implemented in core',
  fetchDisappearingDuration: 'not implemented in core',
  requestPlaceholderResend: 'not implemented in core',
  newsletterUpdate: 'not implemented in core',
  newsletterUpdateDescription: 'not implemented in core',
  newsletterUpdatePicture: 'not implemented in core',
  newsletterRemovePicture: 'not implemented in core',
  newsletterSubscribers: 'not implemented in core',
  updateCoverPhoto: 'core takes an already uploaded photo id, not media',
};

const nativeMethods = () => {
  const src = fs.readFileSync(path.join(__dirname, '../src/lib.rs'), 'utf8');
  const impl = src.slice(src.indexOf('impl RoseBaileysClient'));
  const toCamel = (n) => n.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());
  return new Set([...impl.matchAll(/pub (?:async )?fn ([a-z0-9_]+)\(/g)].map((m) => toCamel(m[1])));
};

test('the socket covers every Baileys socket key or documents why not', () => {
  const sock = createSockApi(new Proxy({}, { get: () => () => {} }), new EventEmitter());
  const missing = BAILEYS_SOCK_KEYS.filter((k) => !(k in sock) && !(k in NOT_EXPOSED));
  assert.deepEqual(missing, [], 'Baileys socket keys neither implemented nor listed in NOT_EXPOSED');

  const stale = Object.keys(NOT_EXPOSED).filter((k) => k in sock || !BAILEYS_SOCK_KEYS.includes(k));
  assert.deepEqual(stale, [], 'NOT_EXPOSED entries that are implemented or not Baileys keys');
});

test('every native call made by the socket exists on the addon', () => {
  const src = fs.readFileSync(path.join(__dirname, '../sock-api.js'), 'utf8');
  const called = new Set([...src.matchAll(/\bclient\.([A-Za-z0-9_]+)\(/g)].map((m) => m[1]));
  const native = nativeMethods();
  const unknown = [...called].filter((n) => !native.has(n));
  assert.deepEqual(unknown, [], 'socket calls methods the native addon does not have');
});

test('Baileys-named calls map arguments to the native order', () => {
  const calls = [];
  const client = new Proxy({}, { get: (_, name) => (...args) => calls.push([name, args]) });
  const sock = createSockApi(client, new EventEmitter());

  sock.communityLinkGroup('g@g.us', 'c@g.us');
  sock.newsletterUnmute('n@newsletter');
  sock.createCallLink('video');
  sock.updateLastSeenPrivacy('contacts');
  sock.groupToggleEphemeral('g@g.us', 86400);

  assert.deepEqual(calls, [
    ['communityLinkSubgroup', ['c@g.us', 'g@g.us']],
    ['newsletterMute', ['n@newsletter', false]],
    ['createCallLink', [true]],
    ['updatePrivacySetting', ['last', 'contacts']],
    ['setChatEphemeral', ['g@g.us', 86400]],
  ]);
});

test('readMessages and star iterate over Baileys keys', async () => {
  const calls = [];
  const client = new Proxy({}, { get: (_, name) => async (...args) => calls.push([name, args]) });
  const sock = createSockApi(client, new EventEmitter());

  await sock.readMessages([
    { remoteJid: 'a@s.whatsapp.net', id: 'M1' },
    { remoteJid: 'g@g.us', id: 'M2', participant: 'p@lid' },
  ]);
  await sock.star('a@s.whatsapp.net', [{ id: 'M1', fromMe: true }], true);

  assert.deepEqual(calls, [
    ['sendReadReceipt', ['a@s.whatsapp.net', 'M1', null]],
    ['sendReadReceipt', ['g@g.us', 'M2', 'p@lid']],
    ['chatModifyStar', ['a@s.whatsapp.net', 'M1', true, true]],
  ]);
});

test('waitForSocketOpen resolves on connection open', async () => {
  const ev = new EventEmitter();
  const sock = createSockApi(new Proxy({}, { get: () => () => {} }), ev);
  const waiting = sock.waitForSocketOpen(1000);
  ev.emit('connection.update', { connection: 'connecting' });
  ev.emit('connection.update', { connection: 'open' });
  assert.deepEqual(await waiting, { connection: 'open' });
  assert.equal(ev.listenerCount('connection.update'), 0, 'listener removed');
});

test('index.d.ts declares every socket key', () => {
  const sock = createSockApi(new Proxy({}, { get: () => () => {} }), new EventEmitter());
  const d = fs.readFileSync(path.join(__dirname, '../index.d.ts'), 'utf8');
  const start = d.indexOf('export interface WASocket {');
  const body = d.slice(start, d.indexOf('\n}\n', start));
  const declared = new Set([...body.matchAll(/^  ([A-Za-z0-9_]+)\??[(:]/gm)].map((m) => m[1]));
  assert.deepEqual(Object.keys(sock).filter((k) => !declared.has(k)), [], 'socket keys missing from index.d.ts');
});
