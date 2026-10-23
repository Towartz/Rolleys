'use strict';

/**
 * Builds the socket object returned by `makeWASocket`: the addon's own helpers plus the
 * Baileys socket API names. Takes the native client and event emitter as arguments so the
 * shape can be tested without loading the native addon.
 */

/** Resolves when `check(update)` is truthy for an `event` emission (Baileys waitForConnectionUpdate). */
function waitFor(ev, event, check, timeoutMs) {
  return new Promise((resolve, reject) => {
    let timer;
    const listener = (update) => {
      if (!check(update)) return;
      clearTimeout(timer);
      ev.off(event, listener);
      resolve(update);
    };
    ev.on(event, listener);
    if (timeoutMs) {
      timer = setTimeout(() => {
        ev.off(event, listener);
        reject(new Error(`Timed out waiting for ${event}`));
      }, timeoutMs);
    }
  });
}

function createSockApi(client, ev) {
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

    // ---- Baileys socket API names (makeWASocket in Baileys 7) ----
    type: 'md',

    waitForConnectionUpdate: (check, timeoutMs) => waitFor(ev, 'connection.update', check, timeoutMs),
    waitForSocketOpen: (timeoutMs) =>
      waitFor(ev, 'connection.update', (u) => u.connection === 'open', timeoutMs),

    logout: () => client.logout(),
    requestPairingCode: (phoneNumber) => client.requestPairingCode(phoneNumber),
    updateProfileName: (name) => client.updateProfileName(name),
    fetchStatus: async (...jids) =>
      Promise.all(jids.flat().map(async (id) => ({ id, status: await client.fetchStatus(id) }))),
    getBusinessProfile: (jid) => client.getBusinessProfile(jid),
    rejectCall: (callId, callFrom) => client.rejectCall(callId, callFrom),
    createCallLink: (type = 'audio') => client.createCallLink(type === 'video'),
    getBotListV2: () => client.getBotListV2(),
    fetchAccountReachoutTimelock: () => client.fetchAccountReachoutTimelock(),
    fetchNewChatMessageCap: () => client.fetchNewChatMessageCap(),
    cleanDirtyBits: (type, fromTimestamp = null) =>
      client.cleanDirtyBits(type, fromTimestamp == null ? null : Number(fromTimestamp)),
    resyncAppState: (collections, isInitialSync = false) =>
      client.resyncAppState([...collections], !!isInitialSync),
    updateMediaMessage: (message) => client.updateMediaMessage(message.key.id),
    getUSyncDevices: async (jids) => {
      const out = [];
      for (const jid of jids) {
        const user = jid.split('@')[0].split(':')[0];
        for (const device of await client.getUserDevices(jid)) {
          out.push({ user, device: Number(device.id ?? device) });
        }
      }
      return out;
    },

    // Read receipts and stars (Baileys keys: { remoteJid, id, participant, fromMe })
    // One receipt per chat/author, `read-self` when our read receipts are off (Baileys readMessages).
    readMessages: (keys) =>
      client.readMessages(
        keys.map((k) => ({
          remoteJid: k.remoteJid,
          id: k.id,
          participant: k.participant || null,
          fromMe: !!k.fromMe,
        }))
      ),
    star: async (jid, messages, star) => {
      for (const m of messages) {
        await client.chatModifyStar(jid, m.id, !!m.fromMe, !!star);
      }
    },
    addLabel: (jid, label) =>
      label.deleted
        ? client.deleteLabel(label.id)
        : client.upsertLabel(label.id || null, label.name, label.color || 0, label.predefinedId || null),

    // Privacy (Baileys WAPrivacyValue strings)
    updateLastSeenPrivacy: (value) => client.updatePrivacySetting('last', value),
    updateOnlinePrivacy: (value) => client.updatePrivacySetting('online', value),
    updateProfilePicturePrivacy: (value) => client.updatePrivacySetting('profile', value),
    updateStatusPrivacy: (value) => client.updatePrivacySetting('status', value),
    updateReadReceiptsPrivacy: (value) => client.updatePrivacySetting('readreceipts', value),
    updateGroupsAddPrivacy: (value) => client.updatePrivacySetting('groupadd', value),
    updateCallPrivacy: (value) => client.updatePrivacySetting('calladd', value),
    updateMessagesPrivacy: (value) => client.updatePrivacySetting('messages', value),
    updateDisableLinkPreviewsPrivacy: (isPreviewsDisabled) =>
      client.updateDisableLinkPreviewsPrivacy(!!isPreviewsDisabled),

    // Groups
    groupGetInviteInfo: (code) => client.groupGetInviteInfo(code),
    groupToggleEphemeral: (jid, ephemeralExpiration) => client.setChatEphemeral(jid, ephemeralExpiration),

    // Products
    productCreate: (create) => client.productCreate(create),
    productUpdate: (productId, update) => client.productUpdate(productId, update),
    productDelete: (productIds) => client.productDelete(productIds),
    removeCoverPhoto: (id) => client.removeCoverPhoto(id),

    // Communities (a community is a group, so group calls apply to it)
    communityCreate: (subject, body) => client.communityCreate(subject, body || null),
    communityCreateGroup: (subject, participants, parentCommunityJid) =>
      client.communityCreateSubgroup(parentCommunityJid, subject, participants),
    communityLeave: (id) => client.communityLeave(id),
    communityLinkGroup: (groupJid, parentCommunityJid) => client.communityLinkSubgroup(parentCommunityJid, groupJid),
    communityUnlinkGroup: (groupJid, parentCommunityJid) => client.communityUnlinkSubgroup(parentCommunityJid, groupJid),
    communityFetchLinkedGroups: (jid) => client.communityFetchSubgroups(jid),
    communityMetadata: (jid) => client.communityGetMetadata(jid),
    communityInviteCode: (jid) => client.communityGetInviteCode(jid),
    communityRevokeInvite: (jid) => client.communityRevokeInvite(jid),
    communityAcceptInvite: (code) => client.communityAcceptInvite(code),
    communityGetInviteInfo: (code) => client.communityGetInviteInfo(code),
    communityToggleEphemeral: (jid, ephemeralExpiration) => client.communityToggleEphemeral(jid, ephemeralExpiration),
    communitySettingUpdate: (jid, setting) => client.communitySettingUpdate(jid, setting),
    communityMemberAddMode: (jid, mode) => client.communityMemberAddMode(jid, mode),
    communityJoinApprovalMode: (jid, mode) => client.communityJoinApprovalMode(jid, mode),
    communityFetchAllParticipating: () => client.communityFetchAllParticipating(),
    communityUpdateSubject: (jid, subject) => client.groupUpdateSubject(jid, subject),
    communityUpdateDescription: (jid, description) => client.groupUpdateDescription(jid, description || ''),
    communityParticipantsUpdate: (jid, participants, action) => client.groupParticipantsUpdate(jid, participants, action),
    communityRequestParticipantsList: (jid) => client.groupRequestParticipantsList(jid),
    communityRequestParticipantsUpdate: (jid, participants, action) =>
      client.groupRequestParticipantsUpdate(jid, participants, action),
    communityRevokeInviteV4: (communityJid, invitedJid) => client.groupRevokeInviteV4(communityJid, invitedJid),
    communityAcceptInviteV4: (key, inviteMessage) =>
      client.groupAcceptInviteV4(
        inviteMessage.groupJid,
        inviteMessage.inviteCode,
        Number(inviteMessage.inviteExpiration),
        typeof key === 'string' ? key : key.remoteJid
      ),

    // Contacts, quick replies, account
    addOrEditContact: (jid, contact) =>
      client.addOrEditContact(jid, {
        fullName: contact.fullName ?? null,
        firstName: contact.firstName ?? null,
        lidJid: contact.lidJid ?? null,
        pnJid: contact.pnJid ?? null,
        username: contact.username ?? null,
        saveOnPrimaryAddressbook: contact.saveOnPrimaryAddressbook ?? null,
      }),
    removeContact: (jid) => client.removeContact(jid),
    addOrEditQuickReply: (quickReply) =>
      client.addOrEditQuickReply(quickReply.timestamp ?? null, quickReply.shortcut || '', quickReply.message || ''),
    removeQuickReply: (timestamp) => client.removeQuickReply(timestamp),
    updateDefaultDisappearingMode: (duration) => client.updateDefaultDisappearingMode(duration),
    fetchDisappearingDuration: async (...jids) => {
      const rows = await client.fetchDisappearingDuration(jids.flat());
      return rows.map((r) => ({ id: r.jid, disappearingMode: { duration: r.durationSecs, setAt: new Date(r.setAt * 1000) } }));
    },
    getOrderDetails: (orderId, tokenBase64) => client.getOrderDetails(orderId, tokenBase64),
    updateBussinesProfile: (args) => client.updateBusinessProfile({
      address: args.address ?? null,
      email: args.email ?? null,
      description: args.description ?? null,
      websites: args.websites ?? null,
      hours: args.hours
        ? {
            timezone: args.hours.timezone,
            days: args.hours.days.map((d) => ({
              day: d.day,
              mode: d.mode,
              openTimeInMinutes: d.openTimeInMinutes ?? null,
              closeTimeInMinutes: d.closeTimeInMinutes ?? null,
            })),
          }
        : null,
    }),
    updateMemberLabel: (jid, memberLabel) => client.updateMemberLabel(jid, memberLabel),
    requestPlaceholderResend: (key) =>
      client.requestPlaceholderResend(key.remoteJid, key.id, !!key.fromMe, key.participant || null),

    // Newsletters (channels)
    newsletterCreate: (name, description) => client.newsletterCreate(name, description || null),
    newsletterMetadata: (type, key) => client.newsletterQueryMetadata(key),
    newsletterFollow: (jid) => client.newsletterFollow(jid),
    newsletterUnfollow: (jid) => client.newsletterUnfollow(jid),
    newsletterMute: (jid) => client.newsletterMute(jid, true),
    newsletterUnmute: (jid) => client.newsletterMute(jid, false),
    newsletterUpdateName: (jid, name) => client.newsletterUpdateName(jid, name),
    newsletterDelete: (jid) => client.newsletterDelete(jid),
    newsletterUpdate: (jid, updates) =>
      client.newsletterUpdate(jid, {
        name: updates.name ?? null,
        description: updates.description ?? null,
        picture: updates.picture ?? null,
      }),
    newsletterUpdateDescription: (jid, description) => client.newsletterUpdateDescription(jid, description),
    newsletterUpdatePicture: (jid, content) => client.newsletterUpdatePicture(jid, Buffer.from(content)),
    newsletterRemovePicture: (jid) => client.newsletterRemovePicture(jid),
    newsletterSubscribers: async (jid) => ({ subscribers: Number(await client.newsletterSubscribers(jid)) }),
    newsletterReactMessage: (jid, serverId, reaction) => client.newsletterReact(jid, Number(serverId), reaction || ''),
    newsletterFetchMessages: (jid, count, since) => client.newsletterFetchMessages(jid, count, since || null),
    newsletterAdminCount: (jid) => client.newsletterAdminCount(jid),
    newsletterChangeOwner: (jid, newOwnerJid) => client.newsletterChangeOwner(jid, newOwnerJid),
    newsletterDemote: (jid, userJid) => client.newsletterDemote(jid, userJid),
    subscribeNewsletterUpdates: (jid) => client.subscribeNewsletterUpdates(jid),
  };
}

module.exports = { createSockApi };
