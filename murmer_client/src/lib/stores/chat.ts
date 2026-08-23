import { writable, get } from 'svelte/store';
import type { Message, SearchResults } from '../types';
import { session } from './session';
import { notify } from '../notify';
import { channelNotifications } from './channelNotifications';
import { soundboardPrefs } from './soundboardSettings';
import { screenShareWindows } from './screenShareWindows';
import { prepareMessage, containsMention, normalizeReactions } from '../message-utils';
import { parseWikiSearchHits } from '../chat/search';
import { WebSocketManager } from '../websocket-manager';
import { connection } from './connection';
import { typing } from './typing';
import { unread } from './unread';
import { threadData } from './thread';
import { dm } from './dm';
import { drafts } from './drafts';
import { pinned } from './pins';
import { peerKeys } from './peerKeys';
import { channelKeys } from './channelKeys';
import { decryptDm, encryptDm } from '../dm-crypto';
import {
  decryptChannelMessage,
  encryptChannelMessage,
  parseSealedMessage,
  type ChannelMessagePayload
} from '../channel-crypto';
import { loadKeyPair } from '../keypair';

/** Maximum number of search results to request from server */
const MAX_SEARCH_RESULTS = 200;
/** Timeout for search requests in milliseconds */
const SEARCH_TIMEOUT_MS = 5000;
/** Timeout for peer key lookups in milliseconds */
const KEY_REQUEST_TIMEOUT_MS = 5000;
/** Minimum interval between typing events sent to the server */
const TYPING_SEND_INTERVAL_MS = 2000;

/** Pending search request tracking */
type PendingSearch = {
  resolve: (results: SearchResults) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

function createChatStore() {
  const { subscribe, update, set } = writable<Message[]>([]);
  const wsManager = new WebSocketManager();
  let requestIdCounter = 1;
  const pendingSearches = new Map<number, PendingSearch>();
  let lastTypingSentAt = 0;
  /** Channels whose messages are end-to-end encrypted, per the channel list. */
  let encryptedChannels = new Set<number>();
  /** The channel this connection is joined to; what `send` seals for. */
  let joinedChannelId = 0;
  /** Last pin snapshot per channel, kept so sealed previews can be re-opened
   *  once the channel key arrives. */
  const rawPins = new Map<number, unknown[]>();

  /** In-flight and resolved peer key lookups, cached per connection. */
  const peerKeyRequests = new Map<string, Promise<string | null>>();
  const pendingKeyRequests = new Map<
    string,
    { resolve: (key: string | null) => void; timeout: ReturnType<typeof setTimeout> }
  >();

  /** Clear all pending search requests with an error */
  function clearPendingSearches(reason: string): void {
    for (const [id, entry] of pendingSearches.entries()) {
      clearTimeout(entry.timeout);
      entry.reject(new Error(reason));
      pendingSearches.delete(id);
    }
  }

  /** Drop peer key lookups from a previous connection. */
  function clearPeerKeyRequests(): void {
    for (const entry of pendingKeyRequests.values()) {
      clearTimeout(entry.timeout);
      entry.resolve(null);
    }
    pendingKeyRequests.clear();
    peerKeyRequests.clear();
  }

  /**
   * Resolve a peer's identity key for DM encryption: asks the server once
   * per connection and records the answer in the pinning store (which flags
   * key changes). Resolves to null when the peer has no key binding (e.g.
   * bots) or the server does not answer in time.
   */
  function fetchPeerKey(peer: string): Promise<string | null> {
    const cached = peerKeyRequests.get(peer);
    if (cached) return cached;
    const request = new Promise<string | null>((resolve) => {
      const timeout = setTimeout(() => {
        pendingKeyRequests.delete(peer);
        peerKeyRequests.delete(peer);
        resolve(null);
      }, KEY_REQUEST_TIMEOUT_MS);
      pendingKeyRequests.set(peer, { resolve, timeout });
      sendRaw({ type: 'get-user-key', user: peer });
    });
    peerKeyRequests.set(peer, request);
    return request;
  }

  /**
   * Decrypt a DM frame from a conversation with `peer`. NaCl box's shared
   * secret is symmetric, so the peer's public key decrypts both directions
   * (their messages and our own echoed/stored ones). Failures surface as a
   * `decryptFailed` placeholder message instead of being dropped.
   */
  async function decryptDmFrame(peer: string, msg: Message): Promise<Message> {
    const nonce = typeof msg.nonce === 'string' ? msg.nonce : null;
    const ciphertext = typeof msg.ciphertext === 'string' ? msg.ciphertext : null;
    let text: string | null = null;
    if (nonce && ciphertext) {
      const key = await fetchPeerKey(peer);
      if (key) text = decryptDm(nonce, ciphertext, key, loadKeyPair().secretKey);
    }
    if (text === null) {
      const failed: Message = { ...msg, decryptFailed: true };
      delete failed.text;
      return prepareMessage(failed);
    }
    return prepareMessage({ ...msg, text });
  }

  /**
   * Open a message from an encrypted channel and fold its payload back into
   * the shape the rest of the app renders (text, image, attachment, reply
   * quote). The sealed envelope is kept on the message so a later key can
   * re-open it; `decryptPending` marks exactly that case, and is what
   * separates "the key has not reached us yet" from `decryptFailed`, which is
   * final.
   *
   * A frame with no `enc` passes through untouched: plaintext channels, and
   * messages a channel carried before encryption was switched on, still
   * render normally.
   */
  function decryptChannelFrame(msg: Message): Message {
    const sealed = parseSealedMessage(msg.enc);
    if (!sealed) return prepareMessage(msg);
    const channelId = typeof msg.channelId === 'number' ? msg.channelId : joinedChannelId;
    const key = channelKeys.keyFor(channelId, sealed.epoch);
    const payload = key ? decryptChannelMessage(sealed, key) : null;
    if (!payload) {
      const failed: Message = { ...msg };
      delete failed.text;
      delete failed.image;
      delete failed.attachment;
      if (key) {
        failed.decryptFailed = true;
      } else {
        failed.decryptPending = true;
      }
      return prepareMessage(failed);
    }
    const opened: Message = { ...msg };
    delete opened.decryptFailed;
    delete opened.decryptPending;
    if (typeof payload.text === 'string') opened.text = payload.text;
    if (typeof payload.image === 'string') opened.image = payload.image;
    if (payload.attachment) opened.attachment = payload.attachment;
    // The server cannot quote an encrypted message, so it sends the reply's id
    // and author with an empty snippet; the snippet travels sealed instead.
    if (opened.replyTo && typeof payload.replyText === 'string') {
      opened.replyTo = { ...opened.replyTo, text: payload.replyText };
    }
    return prepareMessage(opened);
  }

  /**
   * Publish a channel's pins, opening any sealed previews on the way.
   *
   * Pins arrive with the channel join, which on a reconnect is before the
   * channel keys do, so the raw snapshot is kept and re-applied when a key
   * turns up — otherwise the pinned bar would sit empty until the next pin.
   */
  function applyPins(channelId: number, raw: unknown[]): void {
    rawPins.set(channelId, raw);
    pinned.setChannelPins(
      channelId,
      raw.map((item) => {
        if (!item || typeof item !== 'object') return item;
        const sealed = parseSealedMessage((item as Record<string, unknown>).enc);
        if (!sealed) return item;
        const key = channelKeys.keyFor(channelId, sealed.epoch);
        const payload = key ? decryptChannelMessage(sealed, key) : null;
        return { ...(item as Record<string, unknown>), text: payload?.text ?? null };
      })
    );
  }

  /**
   * Re-open every message that was waiting on a key. Runs whenever the key
   * store changes, which is how a message that arrived before its epoch did
   * stops being a placeholder without the user reloading anything.
   */
  function retryPendingDecrypts(): void {
    update((messages) => {
      let changed = false;
      const next = messages.map((item) => {
        if (item.decryptPending !== true) return item;
        const opened = decryptChannelFrame(item);
        if (opened.decryptPending === true) return item;
        changed = true;
        return opened;
      });
      return changed ? next : messages;
    });
    for (const [channelId, raw] of rawPins) applyPins(channelId, raw);
    const thread = get(threadData);
    if (thread && thread.messages.some((item) => item.decryptPending === true)) {
      threadData.set({
        ...thread,
        messages: thread.messages.map((item) =>
          item.decryptPending === true ? decryptChannelFrame(item) : item
        )
      });
    }
  }

  /** Build the sealed envelope for a message in the joined channel, if it is
   *  encrypted. Returns `null` when the channel is plaintext (send as-is) and
   *  `'locked'` when it is encrypted but we hold no key to send under. */
  function sealForJoinedChannel(payload: ChannelMessagePayload): Record<string, unknown> | null | 'locked' {
    if (!encryptedChannels.has(joinedChannelId)) return null;
    const current = channelKeys.currentKey(joinedChannelId);
    if (!current) return 'locked';
    const sealed = encryptChannelMessage(payload, current.epoch, current.key);
    return sealed ? { epoch: sealed.epoch, nonce: sealed.nonce, ciphertext: sealed.ciphertext } : 'locked';
  }

  /** Handle incoming messages from WebSocket */
  function handleMessage(msg: Message): void {
    const current = get(session).user;

    switch (msg.type) {
      case 'chat': {
        const prepared = decryptChannelFrame(msg);
        update((m) => [...m, prepared]);

        // The author's message arriving supersedes their typing signal.
        if (typeof prepared.channelId === 'number' && prepared.user) {
          typing.clear(prepared.channelId, prepared.user);
        }

        // Handle notifications
        if (!current || prepared.user !== current) {
          const chId = prepared.channelId ?? 0;
          const preferences = get(channelNotifications);
          const preference = preferences[chId] ?? 'all';
          const mention = current ? containsMention(prepared.text, current) : false;
          const trimmedText = (prepared.text ?? '').trim();
          const shouldNotify =
            preference === 'all' ? true : preference === 'mentions' ? mention : false;

          if (shouldNotify) {
            if (mention) {
              const body =
                trimmedText.length > 0 ? trimmedText : `${prepared.user ?? 'Someone'} mentioned you`;
              notify(`Mention from ${prepared.user ?? 'Unknown user'}`, body);
            } else {
              const sender = prepared.user ?? 'Unknown user';
              const body = trimmedText.length > 0 ? trimmedText : 'sent a message';
              notify('New message', `${sender}: ${body}`);
            }
          }
        }
        break;
      }

      case 'history': {
        const msgs = ((msg.messages as Message[]) || []).map((item) => decryptChannelFrame(item));
        update((m) => {
          // Drop messages already in the store so overlapping history
          // responses (e.g. after a reconnect) don't duplicate entries.
          const existing = new Set(m.map((item) => item.id).filter((id) => typeof id === 'number'));
          const fresh = msgs.filter((item) => typeof item.id !== 'number' || !existing.has(item.id));
          return [...fresh, ...m];
        });
        break;
      }

      case 'reaction-update': {
        const messageId = msg.messageId as number | undefined;
        if (typeof messageId === 'number') {
          const reactions = normalizeReactions(msg.reactions ?? {});
          update((messages) =>
            messages.map((m) => (m.id === messageId ? { ...m, reactions } : m))
          );
        }
        break;
      }

      case 'message-edited': {
        const messageId = msg.id as number | undefined;
        if (typeof messageId !== 'number') break;
        const editedAt = typeof msg.editedAt === 'string' ? msg.editedAt : undefined;
        const sealed = parseSealedMessage(msg.enc);
        if (sealed) {
          // The edit replaces the sealed payload; re-open it through the same
          // path a fresh message takes, so a missing key leaves a placeholder
          // rather than the pre-edit text.
          update((messages) =>
            messages.map((m) =>
              m.id === messageId
                ? decryptChannelFrame({ ...m, enc: msg.enc, edited: true, editedAt })
                : m
            )
          );
        } else if (typeof msg.text === 'string') {
          const text = msg.text;
          update((messages) =>
            messages.map((m) => (m.id === messageId ? { ...m, text, edited: true, editedAt } : m))
          );
        }
        break;
      }

      case 'messages-purged': {
        // An Owner wiped the server's history from the Danger Zone. Every
        // message the client is holding is gone server-side, including the
        // pins and threads that point at them, so the local copies have to go
        // too — otherwise the channel keeps rendering messages nobody can
        // load, react to or open again.
        set([]);
        threadData.set(null);
        pinned.reset();
        unread.reset();
        break;
      }

      case 'message-deleted': {
        const messageId = (msg.id as number | undefined) ?? (msg.messageId as number | undefined);
        if (typeof messageId === 'number') {
          update((messages) => messages.filter((m) => m.id !== messageId));
        }
        break;
      }

      case 'search-results': {
        const payload = msg as any;
        const requestId = Number(payload.requestId);
        if (!Number.isNaN(requestId)) {
          const pending = pendingSearches.get(requestId);
          if (pending) {
            pendingSearches.delete(requestId);
            clearTimeout(pending.timeout);
            const list: Message[] = Array.isArray(payload.messages)
              ? (payload.messages as Message[])
              : [];
            const prepared = list.map((item) => decryptChannelFrame(item));
            // Wiki pages ride the same frame: one query, two indexes.
            pending.resolve({ messages: prepared, pages: parseWikiSearchHits(payload.pages) });
          }
        }
        break;
      }

      case 'typing': {
        const channelId = msg.channelId as number | undefined;
        const user = msg.user as string | undefined;
        if (typeof channelId === 'number' && typeof user === 'string' && user !== current) {
          typing.bump(channelId, user);
        }
        break;
      }

      case 'pins': {
        const channelId = msg.channelId as number | undefined;
        if (typeof channelId === 'number') {
          applyPins(channelId, Array.isArray(msg.pins) ? (msg.pins as unknown[]) : []);
        }
        break;
      }

      // The channel list is where encryption status arrives, so it is also
      // where key fetching starts and stops.
      case 'channel-list': {
        const list = Array.isArray(msg.channels) ? (msg.channels as unknown[]) : [];
        encryptedChannels = new Set(
          list
            .filter(
              (item): item is Record<string, unknown> =>
                !!item && typeof item === 'object' && (item as Record<string, unknown>).e2ee === true
            )
            .map((item) => item.id)
            .filter((id): id is number => typeof id === 'number')
        );
        channelKeys.syncChannels([...encryptedChannels]);
        break;
      }

      case 'channel-keys': {
        channelKeys.receive(msg as Record<string, unknown>);
        retryPendingDecrypts();
        break;
      }

      // A member handed out or rotated a key. Re-read rather than trusting the
      // notification's contents: wraps are per-recipient and only ours are
      // ours to fetch.
      case 'channel-keys-changed': {
        const channelId = msg.channelId as number | undefined;
        if (typeof channelId === 'number') channelKeys.request(channelId);
        break;
      }

      case 'dm': {
        const from = typeof msg.from === 'string' ? msg.from : null;
        const to = typeof msg.to === 'string' ? msg.to : null;
        if (!from || !to || !current) break;
        const peer = from === current ? to : from;
        void decryptDmFrame(peer, msg).then((prepared) => {
          dm.receive(prepared, current);
          if (from !== current && dm.getActive() !== from) {
            const text = (prepared.text ?? '').trim();
            notify(`Direct message from ${from}`, text || 'sent you a message');
          }
        });
        break;
      }

      case 'dm-history': {
        const peer = typeof msg.with === 'string' ? msg.with : null;
        if (peer) {
          const list: Message[] = Array.isArray(msg.messages) ? (msg.messages as Message[]) : [];
          void Promise.all(list.map((item) => decryptDmFrame(peer, item))).then((prepared) => {
            dm.setHistory(peer, prepared);
          });
        }
        break;
      }

      case 'user-key': {
        const user = typeof msg.user === 'string' ? msg.user : null;
        if (!user) break;
        const pending = pendingKeyRequests.get(user);
        if (!pending) break;
        pendingKeyRequests.delete(user);
        clearTimeout(pending.timeout);
        const key = typeof msg.publicKey === 'string' ? msg.publicKey : null;
        if (key) {
          peerKeys.observe(user, key);
        } else {
          // No binding yet (e.g. a bot, or a user who never connected):
          // don't cache the miss so a later attempt asks again.
          peerKeyRequests.delete(user);
        }
        pending.resolve(key);
        break;
      }

      case 'thread': {
        const rootId = msg.rootId as number | undefined;
        const channelId = msg.channelId as number | undefined;
        if (typeof rootId === 'number' && typeof channelId === 'number') {
          const list: Message[] = Array.isArray(msg.messages) ? (msg.messages as Message[]) : [];
          threadData.set({
            rootId,
            channelId,
            messages: list.map((item) => decryptChannelFrame(item))
          });
        }
        break;
      }

      // Global announcement for messages in every channel; the full message
      // only reaches clients joined to its channel, so unread tracking and
      // cross-channel notifications hang off this event instead.
      case 'message-notify': {
        const channelId = msg.channelId as number | undefined;
        const messageId = msg.id as number | undefined;
        const sender = typeof msg.user === 'string' ? msg.user : undefined;
        if (typeof channelId !== 'number' || typeof messageId !== 'number') break;
        if (current && sender === current) break;
        // Messages for the channel on screen arrive as regular chat events,
        // which already handle notifications there.
        if (channelId === unread.getActive()) break;

        // In an encrypted channel the announcement carries the sealed
        // envelope instead of the text; members hold the key, so mention
        // detection and notification previews keep working locally.
        let text = typeof msg.text === 'string' ? msg.text : '';
        const sealed = parseSealedMessage(msg.enc);
        if (sealed) {
          const key = channelKeys.keyFor(channelId, sealed.epoch);
          const payload = key ? decryptChannelMessage(sealed, key) : null;
          text = typeof payload?.text === 'string' ? payload.text : '';
        }
        const mention = current ? containsMention(text, current) : false;
        unread.recordIncoming(channelId, messageId, mention);

        const preferences = get(channelNotifications);
        const preference = preferences[channelId] ?? 'all';
        const shouldNotify =
          preference === 'all' ? true : preference === 'mentions' ? mention : false;
        if (shouldNotify) {
          const from = sender ?? 'Unknown user';
          const trimmedText = text.trim();
          if (mention) {
            notify(`Mention from ${from}`, trimmedText || `${from} mentioned you`);
          } else {
            notify('New message', `${from}: ${trimmedText || 'sent a message'}`);
          }
        }
        break;
      }

      case 'search-error': {
        const payload = msg as any;
        const requestId = Number(payload.requestId);
        if (!Number.isNaN(requestId)) {
          const pending = pendingSearches.get(requestId);
          if (pending) {
            pendingSearches.delete(requestId);
            clearTimeout(pending.timeout);
            const errorMessage =
              typeof payload.message === 'string' ? payload.message : 'Search failed';
            pending.reject(new Error(errorMessage));
          }
        }
        break;
      }
    }
  }

  /**
   * Connect to a WebSocket server.
   * @param url - WebSocket URL
   * @param onOpen - Optional callback when connection opens
   */
  function connect(url: string, onOpen?: () => void): void {
    set([]); // Clear previous history when connecting to a server
    typing.reset();
    // Per-channel client state (last-read markers, notification preferences)
    // is persisted per server; switch both stores to this server's slice.
    unread.setServer(url);
    channelNotifications.setServer(url);
    // Sound ids and usernames are also only unique per server.
    soundboardPrefs.setServer(url);
    // Screen share windows are remembered per sharer, which is a username too.
    screenShareWindows.setServer(url);
    // Key pins persist per server; in-flight lookups belong to the old one.
    peerKeys.setServer(url);
    // Unsent composer text is parked per server too, so a reconnect gives
    // back what was half-typed instead of discarding it.
    drafts.setServer(url);
    clearPeerKeyRequests();
    // Channel keys are held in memory only — the server keeps the wraps, and
    // they are re-fetched from the channel list this connection sends us.
    channelKeys.reset();
    encryptedChannels = new Set();
    joinedChannelId = 0;
    rawPins.clear();
    unread.reset();
    threadData.set(null);
    dm.reset();
    pinned.reset();
    connection.set('connecting');
    wsManager.connect(
      url,
      handleMessage,
      () => {
        connection.set('connected');
        onOpen?.();
      },
      (info) => {
        clearPendingSearches('Connection closed');
        clearPeerKeyRequests();
        // Intentional closes (leaving the server, reconnecting) update the
        // state themselves; everything else is a failure to surface.
        if (!info.intentional) {
          connection.set(info.opened ? 'disconnected' : 'failed');
        }
      },
      () => clearPendingSearches('WebSocket error')
    );
    // connect() is a no-op when already connected to the same URL; reflect that.
    if (wsManager.isConnected()) {
      connection.set('connected');
    }
  }

  /**
   * Mark the connection as lost when the server stops responding even though
   * the socket still looks open (e.g. network dropped without a close frame).
   */
  function connectionLost(): void {
    if (!wsManager.isConnected()) return;
    wsManager.disconnect();
    clearPendingSearches('Connection lost');
    clearPeerKeyRequests();
    connection.set('disconnected');
  }

  /**
   * Send a chat message, sealing it first when the joined channel is
   * end-to-end encrypted.
   *
   * `content` is everything that must stay off the server in an encrypted
   * channel (text, an image URL, an attachment); `extra` is the metadata that
   * has to stay readable for the server to route and thread the message.
   * Returns an error string for the caller to surface, or null on success.
   */
  function sendMessage(
    user: string,
    content: ChannelMessagePayload,
    extra: Record<string, unknown> = {}
  ): string | null {
    if (!wsManager.isConnected()) return 'Not connected to the server.';

    const now = new Date();
    const payload: Record<string, unknown> = {
      type: 'chat',
      user,
      time: now.toLocaleTimeString(),
      timestamp: now.toISOString(),
      ...extra
    };

    const sealed = sealForJoinedChannel(content);
    if (sealed === 'locked') {
      return 'This channel is encrypted and your copy of its key has not arrived yet.';
    }
    if (sealed) {
      payload.enc = sealed;
    } else {
      Object.assign(payload, content);
      delete payload.replyText;
    }
    wsManager.send(payload);
    return null;
  }

  /**
   * Send a chat message.
   * @param user - Username
   * @param text - Message text
   * @param replyTo - Optional ID of the message being replied to
   * @param replyText - Quoted snippet, needed only in encrypted channels where
   *   the server has no plaintext to quote from
   * @returns null on success, or an error message for the caller to surface
   */
  function send(user: string, text: string, replyTo?: number, replyText?: string): string | null {
    const extra: Record<string, unknown> = {};
    if (typeof replyTo === 'number' && Number.isFinite(replyTo)) {
      extra.replyTo = replyTo;
    }
    return sendMessage(user, { text, replyText }, extra);
  }

  /**
   * Send an uploaded image or file. The bytes live on the server either way;
   * in an encrypted channel the reference to them travels sealed, so who
   * shared what is not readable from the message store.
   * @returns null on success, or an error message for the caller to surface
   */
  function sendUpload(
    user: string,
    content: { image?: string; attachment?: { url: string; name: string; size: number } }
  ): string | null {
    return sendMessage(user, content);
  }

  /**
   * Switch the connection to a channel.
   * @param channelId - Channel to join
   * @param announce - False records the channel the server already placed us
   *   in (the default channel right after presence) without a redundant join.
   */
  function join(channelId: number, announce = true): void {
    joinedChannelId = channelId;
    if (announce) sendRaw({ type: 'join', channelId });
  }

  /**
   * Signal that the user is typing in the current channel. Throttled so
   * keystrokes don't turn into a message flood.
   */
  function sendTyping(): void {
    if (!wsManager.isConnected()) return;
    const now = Date.now();
    if (now - lastTypingSentAt < TYPING_SEND_INTERVAL_MS) return;
    lastTypingSentAt = now;
    wsManager.send({ type: 'typing' });
  }

  /**
   * Encrypt and send a direct message to another user.
   * @param to - Recipient username
   * @param text - Message text
   * @returns null on success, or an error message for the caller to surface
   */
  async function sendDm(to: string, text: string): Promise<string | null> {
    if (!wsManager.isConnected()) return 'Not connected to the server.';
    const key = await fetchPeerKey(to);
    if (!key) {
      return `${to} has no encryption key on this server and cannot receive direct messages.`;
    }
    if (peerKeys.hasConflict(to)) {
      return `${to}'s security key has changed. Review the warning in the conversation before sending.`;
    }
    const payload = encryptDm(text, key, loadKeyPair().secretKey);
    if (!payload) return 'The message could not be encrypted.';
    const now = new Date();
    wsManager.send({
      type: 'dm',
      to,
      nonce: payload.nonce,
      ciphertext: payload.ciphertext,
      time: now.toLocaleTimeString(),
      timestamp: now.toISOString()
    });
    return null;
  }

  /**
   * Load the direct message history with another user.
   * @param peer - Username of the other participant
   * @param before - Optional message ID to load messages before
   */
  function loadDmHistory(peer: string, before?: number): void {
    sendRaw({ type: 'load-dm-history', with: peer, before });
  }

  /**
   * Load all messages belonging to a thread.
   * @param rootId - ID of the thread's root message
   */
  function loadThread(rootId: number): void {
    if (typeof rootId !== 'number' || Number.isNaN(rootId)) return;
    sendRaw({ type: 'load-thread', rootId });
  }

  /**
   * Send an ephemeral (self-destructing) chat message.
   * @param user - Username
   * @param text - Message text
   * @param expiresAt - ISO 8601 expiry timestamp
   */
  function sendEphemeral(user: string, text: string, expiresAt: string): string | null {
    return sendMessage(user, { text }, { ephemeral: true, expiresAt });
  }

  /**
   * Send a raw message object.
   * @param data - Message data to send
   */
  function sendRaw(data: any): void {
    wsManager.send(data);
  }

  /**
   * Load message history for a channel.
   * @param channelId - Channel ID
   * @param before - Optional message ID to load messages before
   * @param limit - Number of messages to load (default: 50)
   */
  function loadHistory(channelId: number, before?: number, limit = 50): void {
    sendRaw({ type: 'load-history', channelId, before, limit });
  }

  /**
   * React to a message with an emoji.
   * @param messageId - Message ID
   * @param emoji - Emoji to add/remove
   * @param action - 'add' or 'remove'
   */
  function react(messageId: number, emoji: string, action: 'add' | 'remove'): void {
    if (!wsManager.isConnected()) return;
    if (typeof messageId !== 'number' || Number.isNaN(messageId)) return;

    const trimmed = emoji.trim();
    if (!trimmed) return;

    const payload = { type: 'react', messageId, emoji: trimmed, action };
    wsManager.send(payload);
  }

  /**
   * Search a channel: its message history and its wiki pages, which the
   * server answers on one frame from two full-text indexes.
   * @param channelId - Channel ID to search
   * @param query - Search query
   * @param limit - Maximum results (default: 50, max: 200)
   * @returns Promise resolving to the matching messages and wiki pages
   */
  function search(channelId: number, query: string, limit = 50): Promise<SearchResults> {
    if (!wsManager.isConnected()) {
      return Promise.reject(new Error('Not connected to server'));
    }

    const trimmedQuery = query.trim();
    if (!trimmedQuery) {
      return Promise.resolve({ messages: [], pages: [] });
    }

    const boundedLimit = Math.min(Math.max(Math.floor(limit), 1), MAX_SEARCH_RESULTS);
    const requestId = requestIdCounter++;

    return new Promise<SearchResults>((resolve, reject) => {
      const timeout = setTimeout(() => {
        if (pendingSearches.delete(requestId)) {
          reject(new Error('Search timed out'));
        }
      }, SEARCH_TIMEOUT_MS);

      pendingSearches.set(requestId, { resolve, reject, timeout });

      const payload = {
        type: 'search-history',
        channelId,
        query: trimmedQuery,
        limit: boundedLimit,
        requestId
      };
      wsManager.send(payload);
    });
  }

  /**
   * Edit a previously sent message.
   * @param messageId - Message ID to edit
   * @param text - Replacement message text
   */
  function edit(messageId: number, text: string): string | null {
    if (!wsManager.isConnected()) return 'Not connected to the server.';
    if (typeof messageId !== 'number' || Number.isNaN(messageId)) return null;

    const trimmed = text.trim();
    if (!trimmed) return null;

    const sealed = sealForJoinedChannel({ text });
    if (sealed === 'locked') {
      return 'This channel is encrypted and your copy of its key has not arrived yet.';
    }
    wsManager.send(
      sealed ? { type: 'edit-message', messageId, enc: sealed } : { type: 'edit-message', messageId, text }
    );
    return null;
  }

  /**
   * Delete a message.
   * @param messageId - Message ID to delete
   */
  function deleteMessage(messageId: number): void {
    if (!wsManager.isConnected()) return;
    if (typeof messageId !== 'number' || Number.isNaN(messageId)) return;

    const payload = { type: 'delete-message', messageId };
    wsManager.send(payload);
  }

  /**
   * Disconnect from the WebSocket server.
   */
  function disconnect(): void {
    wsManager.disconnect();
    set([]);
    typing.reset();
    unread.reset();
    threadData.set(null);
    dm.reset();
    pinned.reset();
    channelKeys.reset();
    encryptedChannels = new Set();
    joinedChannelId = 0;
    rawPins.clear();
    clearPendingSearches('Disconnected');
    clearPeerKeyRequests();
    connection.set('idle');
  }

  /**
   * Register a handler for a specific message type.
   * @param type - Message type
   * @param callback - Handler function
   */
  function on(type: string, callback: (msg: Message) => void): void {
    wsManager.on(type, callback);
  }

  /**
   * Unregister a handler for a specific message type.
   * @param type - Message type
   * @param callback - Optional specific callback to remove
   */
  function off(type: string, callback?: (msg: Message) => void): void {
    wsManager.off(type, callback);
  }

  // The key store talks to the server through this connection; it deliberately
  // does not import the chat store back, so the dependency runs one way.
  channelKeys.setTransport(sendRaw);

  return {
    subscribe,
    connect,
    connectionLost,
    join,
    send,
    sendUpload,
    sendDm,
    sendEphemeral,
    sendTyping,
    sendRaw,
    loadHistory,
    loadDmHistory,
    loadThread,
    react,
    search,
    edit,
    delete: deleteMessage,
    on,
    off,
    disconnect,
    clear: () => set([])
  };
}

export const chat = createChatStore();
