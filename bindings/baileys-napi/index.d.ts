import { EventEmitter } from 'events';

export interface WASocketConfig {
  databasePath?: string;
  authPath?: string;
  phoneNumber?: string;
  phone?: string;
}

export interface AnyMessageContent {
  text?: string;
  image?: Buffer | Uint8Array;
  video?: Buffer | Uint8Array;
  audio?: Buffer | Uint8Array;
  document?: Buffer | Uint8Array;
  sticker?: Buffer | Uint8Array;
  caption?: string;
  fileName?: string;
  mimetype?: string;
  ptt?: boolean;
  seconds?: number;
  gifPlayback?: boolean;
  isAnimated?: boolean;
  location?: {
    degreesLatitude: number;
    degreesLongitude: number;
    name?: string | null;
    address?: string | null;
    url?: string | null;
  };
  liveLocation?: {
    degreesLatitude: number;
    degreesLongitude: number;
    accuracyInMeters?: number | null;
    caption?: string | null;
  };
  contact?: {
    displayName: string;
    phoneNumber: string;
    vcard?: string | null;
  };
  groupInvite?: {
    groupJid: string;
    inviteCode: string;
    inviteExpiration: number;
    groupName: string;
    caption?: string | null;
  };
  product?: {
    productId?: string;
    id?: string;
    businessOwnerJid?: string;
    title: string;
    priceAmount1000?: number;
    price1000?: number;
    currency?: string;
    caption?: string | null;
  };
  order?: {
    orderId?: string;
    id?: string;
    sellerJid?: string;
    title?: string;
    itemCount?: number;
    totalAmount1000?: number;
    total1000?: number;
    currency?: string;
    message?: string | null;
  };
  viewOnce?: boolean;
}

export interface MessageOptions {
  quoted?: {
    key: {
      id: string;
      remoteJid?: string;
      participant?: string;
    };
    summary?: string;
  };
}

export interface StoredChat {
  jid: string;
  name?: string | null;
  unread_count: number;
  last_message_timestamp: number;
  last_message_summary?: string | null;
  ephemeral_expiration?: number;
  ephemeral_setting_timestamp?: number | null;
}

export interface StoredMessage {
  id: string;
  chat_jid: string;
  sender_jid: string;
  from_me: boolean;
  timestamp: number;
  status: string;
  has_media: boolean;
  summary: string;
  is_revoked?: boolean;
  is_edited?: boolean;
}

export interface StoredContact {
  jid: string;
  name?: string | null;
  push_name?: string | null;
  lid?: string | null;
}

export interface Label {
  id: string;
  name: string;
  color: number;
  predefined_id?: number | null;
  deleted: boolean;
}

export interface CatalogProduct {
  id: string;
  biz_jid: string;
  title: string;
  description?: string | null;
  currency: string;
  price_amount_1000: number;
  sale_price_amount_1000?: number | null;
  retailer_id?: string | null;
  url?: string | null;
  image_urls: string[];
  is_hidden: boolean;
  availability: string;
}

export interface CatalogCollection {
  id: string;
  biz_jid: string;
  name: string;
  products_count: number;
  status: string;
}

export interface CatalogResult {
  products: CatalogProduct[];
  cursor?: string | null;
}

export interface CollectionsResult {
  collections: CatalogCollection[];
  cursor?: string | null;
}

export interface Order {
  order_id: string;
  chat_jid: string;
  seller_jid: string;
  item_count: number;
  status: string;
  surface?: string | null;
  order_title?: string | null;
  message?: string | null;
  total_amount_1000: number;
  total_currency_code: string;
  token?: string | null;
  created_at: number;
}

export interface GroupParticipant {
  jid: string;
  phoneNumber?: string | null;
  lid?: string | null;
  admin?: string | null;
}

export interface GroupMetadata {
  id: string;
  subject: string;
  subjectOwner?: string | null;
  subjectTime?: number | null;
  size: number;
  creation: number;
  owner?: string | null;
  description?: string | null;
  descriptionId?: string | null;
  restrict: boolean;
  announce: boolean;
  isCommunity: boolean;
  isCommunityAnnounce: boolean;
  joinApprovalMode: boolean;
  memberAddMode: string;
  ephemeralDuration?: number | null;
  participants: GroupParticipant[];
}

export interface GroupJoinRequest {
  jid: string;
  requestedTime: number;
  requestMethod?: string | null;
}

export interface DeviceRecord {
  id: number;
  keyIndex?: number | null;
}

export interface OnWhatsAppResult {
  jid: string;
  exists: boolean;
  phoneNumber: string;
}

export interface ChatModification {
  mute?: number | null;
  pin?: boolean;
  archive?: boolean;
  star?: {
    messages: Array<{ id: string; fromMe?: boolean }>;
    star: boolean;
  };
  clear?: boolean | { keepStarred?: boolean };
  delete?: boolean;
  markRead?: boolean;
  unread?: boolean;
}

export interface WASocket {
  ev: EventEmitter;
  client: any;

  connect(): Promise<void>;
  disconnect(): void;
  end(): void;
  isConnected(): boolean;
  getMe(): Promise<string | null>;
  ping(): Promise<number>;

  sendMessage(jid: string, content: string | AnyMessageContent, options?: MessageOptions): Promise<string>;
  sendReaction(jid: string, msgId: string, emoji: string): Promise<string>;
  sendReadReceipt(jid: string, msgId: string, participant?: string): Promise<void>;
  sendPresenceUpdate(presence: string, toJid?: string): Promise<void>;
  downloadMedia(msgId: string, destDir: string): Promise<string>;
  setAutoDownload(enabled: boolean): void;

  getChats(): Promise<StoredChat[]>;
  getMessages(chatJid: string, limit?: number, beforeTimestamp?: number): Promise<StoredMessage[]>;
  getContact(jid: string): Promise<StoredContact | null>;

  groupMetadata(jid: string): Promise<any>;
  groupCreate(subject: string, participants: string[]): Promise<string>;
  groupParticipantsUpdate(jid: string, participants: string[], action: 'add' | 'remove' | 'promote' | 'demote'): Promise<Array<{ jid: string; status: string }>>;
  groupUpdateSubject(jid: string, subject: string): Promise<void>;
  groupUpdateDescription(jid: string, desc: string): Promise<void>;
  groupInviteCode(jid: string): Promise<string>;
  groupAcceptInvite(code: string): Promise<string>;
  groupLeave(jid: string): Promise<void>;
  groupSettingUpdate(jid: string, setting: 'announcement' | 'not_announcement' | 'locked' | 'unlocked'): Promise<void>;
  groupRevokeInvite(jid: string): Promise<string>;
  groupFetchMetadata(jid: string): Promise<GroupMetadata>;
  groupFetchAllParticipating(): Promise<GroupMetadata[]>;
  groupJoinApprovalMode(jid: string, mode: 'on' | 'off'): Promise<void>;
  groupRequestParticipantsList(jid: string): Promise<GroupJoinRequest[]>;
  groupRequestParticipantsUpdate(jid: string, participants: string[], action: 'approve' | 'reject'): Promise<Array<{ jid: string; status: string }>>;
  groupMemberAddMode(jid: string, mode: 'admin_add' | 'all_member_add'): Promise<void>;
  groupRevokeInviteV4(groupJid: string, invitedJid: string): Promise<void>;
  groupAcceptInviteV4(groupJid: string, inviteCode: string, inviteExpiration: number, adminJid: string): Promise<string>;

  profilePictureUrl(jid: string, type?: 'preview' | 'image'): Promise<string | null>;
  updateProfilePicture(jid: string | null, image: Buffer | Uint8Array): Promise<void>;
  removeProfilePicture(jid?: string | null): Promise<void>;
  presenceSubscribe(jid: string): Promise<void>;
  updateProfileStatus(status: string): Promise<void>;
  fetchPrivacySettings(): Promise<Record<string, string>>;
  updatePrivacySetting(category: string, value: string): Promise<void>;
  fetchBlocklist(): Promise<string[]>;
  updateBlockStatus(jid: string, action: 'block' | 'unblock'): Promise<void>;

  editMessage(jid: string, targetMessageId: string, newText: string): Promise<string>;
  revokeMessage(jid: string, targetMessageId: string): Promise<string>;
  setChatEphemeral(jid: string, ephemeralExpiration: number): Promise<void>;
  getChatEphemeral(jid: string): Promise<number | null>;

  getLabels(): Promise<Label[]>;
  getLabel(id: string): Promise<Label | null>;
  upsertLabel(name: string, color?: number, id?: string | null, predefinedId?: number | null): Promise<string>;
  deleteLabel(id: string): Promise<void>;
  addChatLabel(jid: string, labelId: string): Promise<void>;
  removeChatLabel(jid: string, labelId: string): Promise<void>;
  getChatLabels(jid: string): Promise<string[]>;
  getChatsByLabel(labelId: string): Promise<string[]>;
  addMessageLabel(jid: string, msgId: string, labelId: string): Promise<void>;
  removeMessageLabel(jid: string, msgId: string, labelId: string): Promise<void>;
  getMessageLabels(jid: string, msgId: string): Promise<string[]>;
  getMessagesByLabel(labelId: string): Promise<string[]>;

  sendLocation(jid: string, latitude: number, longitude: number, name?: string | null, address?: string | null, url?: string | null): Promise<string>;
  sendLiveLocation(jid: string, latitude: number, longitude: number, accuracy?: number | null, caption?: string | null): Promise<string>;
  sendContact(jid: string, displayName: string, phoneNumber: string): Promise<string>;
  sendVCard(jid: string, displayName: string, vcard: string): Promise<string>;
  sendGroupInvite(jid: string, groupJid: string, inviteCode: string, inviteExpiration: number, groupName: string, caption?: string | null): Promise<string>;

  getCatalog(bizJid: string, limit?: number, cursor?: string | null): Promise<CatalogResult>;
  getProduct(bizJid: string, productId: string): Promise<CatalogProduct>;
  getCollections(bizJid: string, limit?: number, cursor?: string | null): Promise<CollectionsResult>;
  sendProductMessage(toJid: string, bizOwnerJid: string, productId: string, title: string, priceAmount1000: number, currency?: string, caption?: string | null): Promise<string>;
  sendOrderMessage(toJid: string, sellerJid: string, orderId: string, title: string, itemCount: number, totalAmount1000: number, currency?: string, message?: string | null): Promise<string>;
  getCachedOrders(chatJid?: string, limit?: number): Promise<Order[]>;
  getCachedCatalog(bizJid: string): Promise<CatalogProduct[]>;
  getCachedCollections(bizJid: string): Promise<CatalogCollection[]>;

  getUserDevices(userJid: string): Promise<number[]>;
  getPairedDevices(): Promise<DeviceRecord[]>;
  removeCompanionDevice(deviceId: number): Promise<void>;
  onWhatsApp(...numbers: (string | string[])[]): Promise<OnWhatsAppResult[]>;

  chatModify(mod: ChatModification, jid: string): Promise<void>;
  chatModifyClear(jid: string, keepStarred?: boolean): Promise<void>;
  chatModifyDelete(jid: string): Promise<void>;
  chatModifyUnread(jid: string, unread?: boolean): Promise<void>;
  chatModifyPin(jid: string, pin?: boolean): Promise<void>;
  chatModifyMute(jid: string, durationSecs?: number | null): Promise<void>;
  chatModifyArchive(jid: string, archive?: boolean): Promise<void>;
  chatModifyStar(jid: string, msgId: string, fromMe?: boolean, star?: boolean): Promise<void>;
  fetchMessageHistory(jid: string, count?: number, oldestMsgId?: string | null, oldestFromMe?: boolean, oldestTimestamp?: number | null): Promise<string>;

  sendVoiceNote(jid: string, audioBuffer: Buffer | Uint8Array, seconds?: number, waveform?: Buffer | Uint8Array | null): Promise<string>;
  sendDocument(jid: string, docBuffer: Buffer | Uint8Array, fileName: string, mimetype?: string, caption?: string | null, pageCount?: number | null): Promise<string>;
  sendVideo(jid: string, videoBuffer: Buffer | Uint8Array, caption?: string | null, isGif?: boolean, seconds?: number | null): Promise<string>;
  sendSticker(jid: string, stickerBuffer: Buffer | Uint8Array, isAnimated?: boolean): Promise<string>;

  // ---- Baileys socket API names ----
  type: 'md';
  waitForConnectionUpdate(check: (update: any) => boolean | undefined, timeoutMs?: number): Promise<any>;
  waitForSocketOpen(timeoutMs?: number): Promise<any>;
  logout(): Promise<void>;
  requestPairingCode(phoneNumber: string): Promise<string>;
  updateProfileName(name: string): Promise<void>;
  fetchStatus(...jids: string[]): Promise<{ id: string; status: any }[]>;
  getBusinessProfile(jid: string): Promise<any>;
  rejectCall(callId: string, callFrom: string): Promise<void>;
  createCallLink(type?: 'audio' | 'video'): Promise<string | null>;
  getBotListV2(): Promise<any[]>;
  fetchAccountReachoutTimelock(): Promise<any>;
  fetchNewChatMessageCap(): Promise<any>;
  cleanDirtyBits(type: 'account_sync' | 'groups', fromTimestamp?: number | string): Promise<void>;
  resyncAppState(collections: readonly string[], isInitialSync?: boolean): Promise<{ collectionsSynced: number; mutationsApplied: number; collectionsFailed: number }>;
  updateMediaMessage(message: { key: { id: string } }): Promise<void>;
  getUSyncDevices(jids: string[]): Promise<{ user: string; device: number }[]>;
  readMessages(keys: { remoteJid: string; id: string; participant?: string | null; fromMe?: boolean }[]): Promise<void>;
  star(jid: string, messages: { id: string; fromMe?: boolean }[], star: boolean): Promise<void>;
  addLabel(jid: string, label: { id?: string; name: string; color?: number; deleted?: boolean; predefinedId?: string }): Promise<any>;

  updateLastSeenPrivacy(value: string): Promise<void>;
  updateOnlinePrivacy(value: string): Promise<void>;
  updateProfilePicturePrivacy(value: string): Promise<void>;
  updateStatusPrivacy(value: string): Promise<void>;
  updateReadReceiptsPrivacy(value: string): Promise<void>;
  updateGroupsAddPrivacy(value: string): Promise<void>;
  updateCallPrivacy(value: string): Promise<void>;
  updateMessagesPrivacy(value: string): Promise<void>;
  updateDisableLinkPreviewsPrivacy(isPreviewsDisabled: boolean): Promise<void>;

  groupGetInviteInfo(code: string): Promise<GroupMetadata>;
  groupToggleEphemeral(jid: string, ephemeralExpiration: number): Promise<void>;

  productCreate(create: CatalogProduct): Promise<string>;
  productUpdate(productId: string, update: CatalogProduct): Promise<void>;
  productDelete(productIds: string[]): Promise<any>;
  removeCoverPhoto(id: string): Promise<void>;

  communityCreate(subject: string, body?: string): Promise<any>;
  communityCreateGroup(subject: string, participants: string[], parentCommunityJid: string): Promise<string>;
  communityLeave(id: string): Promise<void>;
  communityLinkGroup(groupJid: string, parentCommunityJid: string): Promise<void>;
  communityUnlinkGroup(groupJid: string, parentCommunityJid: string): Promise<void>;
  communityFetchLinkedGroups(jid: string): Promise<any[]>;
  communityMetadata(jid: string): Promise<any>;
  communityInviteCode(jid: string): Promise<string>;
  communityRevokeInvite(jid: string): Promise<string>;
  communityAcceptInvite(code: string): Promise<string>;
  communityGetInviteInfo(code: string): Promise<any>;
  communityToggleEphemeral(jid: string, ephemeralExpiration: number): Promise<void>;
  communitySettingUpdate(jid: string, setting: 'announcement' | 'not_announcement' | 'locked' | 'unlocked'): Promise<void>;
  communityMemberAddMode(jid: string, mode: 'admin_add' | 'all_member_add'): Promise<void>;
  communityJoinApprovalMode(jid: string, mode: 'on' | 'off'): Promise<void>;
  communityFetchAllParticipating(): Promise<string[]>;
  communityUpdateSubject(jid: string, subject: string): Promise<void>;
  communityUpdateDescription(jid: string, description?: string): Promise<void>;
  communityParticipantsUpdate(jid: string, participants: string[], action: 'add' | 'remove' | 'promote' | 'demote'): Promise<any>;
  communityRequestParticipantsList(jid: string): Promise<any>;
  communityRequestParticipantsUpdate(jid: string, participants: string[], action: 'approve' | 'reject'): Promise<any>;
  communityRevokeInviteV4(communityJid: string, invitedJid: string): Promise<any>;
  communityAcceptInviteV4(key: string | { remoteJid: string }, inviteMessage: { groupJid: string; inviteCode: string; inviteExpiration: number | string }): Promise<any>;

  newsletterCreate(name: string, description?: string): Promise<any>;
  newsletterMetadata(type: 'invite' | 'jid', key: string): Promise<any>;
  newsletterFollow(jid: string): Promise<void>;
  newsletterUnfollow(jid: string): Promise<void>;
  newsletterMute(jid: string): Promise<void>;
  newsletterUnmute(jid: string): Promise<void>;
  newsletterUpdateName(jid: string, name: string): Promise<void>;
  newsletterDelete(jid: string): Promise<void>;
  newsletterReactMessage(jid: string, serverId: string, reaction?: string): Promise<void>;
  newsletterFetchMessages(jid: string, count: number, since?: number): Promise<any[]>;
  newsletterAdminCount(jid: string): Promise<number>;
  newsletterChangeOwner(jid: string, newOwnerJid: string): Promise<void>;
  newsletterDemote(jid: string, userJid: string): Promise<void>;
  subscribeNewsletterUpdates(jid: string): Promise<any>;
}

export function makeWASocket(config?: WASocketConfig): WASocket;
export default makeWASocket;

export function jidNormalizedUser(jid: string): string;
export function isJidGroup(jid: string): boolean;
export function isJidBroadcast(jid: string): boolean;
export function isJidStatusBroadcast(jid: string): boolean;
export function isLidUser(jid: string): boolean;
export function jidDecode(jid: string): { user: string; server: string; device?: number; domainType?: number } | undefined;

export const DisconnectReason: {
  connectionClosed: number;
  connectionLost: number;
  connectionReplaced: number;
  timedOut: number;
  loggedOut: number;
  badSession: number;
  restartRequired: number;
  multicastTimeout: number;
};

/** Same as Baileys' Browsers: [platform, browser, version]. */
export const Browsers: {
  ubuntu(browser: string): [string, string, string];
  macOS(browser: string): [string, string, string];
  baileys(browser: string): [string, string, string];
  windows(browser: string): [string, string, string];
  android(browser: string): [string, string, string];
  appropriate(browser: string): [string, string, string];
};

export const WAMessageStatus: {
  ERROR: number;
  PENDING: number;
  SERVER_ACK: number;
  DELIVERY_ACK: number;
  READ: number;
  PLAYED: number;
};

