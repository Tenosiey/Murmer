import { writable, get } from 'svelte/store';
import type { Message } from '../types';
import { session } from './session';
import { notify } from '../notify';
import { channelNotifications } from './channelNotifications';
import { prepareMessage, containsMention, normalizeReactions } from '../message-utils';
import { WebSocketManager } from '../websocket-manager';
import { connection } from './connection';
import { typing } from './typing';
import { unread } from './unread';
import { threadData } from './thread';
import { dm } from './dm';
import { pinned } from './pins';

/** Maximum number of search results to request from server */
const MAX_SEARCH_RESULTS = 200;
/** Timeout for search requests in milliseconds */
const SEARCH_TIMEOUT_MS = 5000;
/** Minimum interval between typing events sent to the server */
const TYPING_SEND_INTERVAL_MS = 2000;

/** Pending search request tracking */
type PendingSearch = {
  resolve: (messages: Message[]) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

function createChatStore() {
  const { subscribe, update, set } = writable<Message[]>([]);
  const wsManager = new WebSocketManager();
  let requestIdCounter = 1;
  const pendingSearches = new Map<number, PendingSearch>();
  let lastTypingSentAt = 0;

  /** Clear all pending search requests with an error */
  function clearPendingSearches(reason: string): void {
    for (const [id, entry] of pendingSearches.entries()) {
      clearTimeout(entry.timeout);
      entry.reject(new Error(reason));
      pendingSearches.delete(id);
    }
  }

  /** Handle incoming messages from WebSocket */
  function handleMessage(msg: Message): void {
    const current = get(session).user;

    switch (msg.type) {
      case 'chat': {
        const prepared = prepareMessage(msg);
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
        const msgs = ((msg.messages as Message[]) || []).map((item) => prepareMessage(item));
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
        if (typeof messageId === 'number' && typeof msg.text === 'string') {
          const text = msg.text;
          const editedAt = typeof msg.editedAt === 'string' ? msg.editedAt : undefined;
          update((messages) =>
            messages.map((m) => (m.id === messageId ? { ...m, text, edited: true, editedAt } : m))
          );
        }
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
            const prepared = list.map((item) => prepareMessage(item));
            pending.resolve(prepared);
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
          const list = Array.isArray(msg.pins) ? (msg.pins as unknown[]) : [];
          pinned.setChannelPins(channelId, list);
        }
        break;
      }

      case 'dm': {
        const prepared = prepareMessage(msg);
        dm.receive(prepared, current);
        const from = typeof prepared.from === 'string' ? prepared.from : null;
        if (from && from !== current && dm.getActive() !== from) {
          const text = (prepared.text ?? '').trim();
          notify(`Direct message from ${from}`, text || 'sent you a message');
        }
        break;
      }

      case 'dm-history': {
        const peer = typeof msg.with === 'string' ? msg.with : null;
        if (peer) {
          const list: Message[] = Array.isArray(msg.messages) ? (msg.messages as Message[]) : [];
          dm.setHistory(peer, list.map((item) => prepareMessage(item)));
        }
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
            messages: list.map((item) => prepareMessage(item))
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

        const text = typeof msg.text === 'string' ? msg.text : '';
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
    connection.set('disconnected');
  }

  /**
   * Send a chat message.
   * @param user - Username
   * @param text - Message text
   * @param replyTo - Optional ID of the message being replied to
   */
  function send(user: string, text: string, replyTo?: number): void {
    if (!wsManager.isConnected()) return;

    const now = new Date();
    const payload: Record<string, unknown> = {
      type: 'chat',
      user,
      text,
      time: now.toLocaleTimeString(),
      timestamp: now.toISOString()
    };
    if (typeof replyTo === 'number' && Number.isFinite(replyTo)) {
      payload.replyTo = replyTo;
    }
    wsManager.send(payload);
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
   * Send a direct message to another user.
   * @param to - Recipient username
   * @param text - Message text
   */
  function sendDm(to: string, text: string): void {
    if (!wsManager.isConnected()) return;
    const now = new Date();
    wsManager.send({
      type: 'dm',
      to,
      text,
      time: now.toLocaleTimeString(),
      timestamp: now.toISOString()
    });
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
  function sendEphemeral(user: string, text: string, expiresAt: string): void {
    if (!wsManager.isConnected()) return;

    const now = new Date();
    const payload = {
      type: 'chat',
      user,
      text,
      time: now.toLocaleTimeString(),
      timestamp: now.toISOString(),
      ephemeral: true,
      expiresAt
    };
    wsManager.send(payload);
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
   * Search chat history.
   * @param channelId - Channel ID to search
   * @param query - Search query
   * @param limit - Maximum results (default: 50, max: 200)
   * @returns Promise resolving to matching messages
   */
  function search(channelId: number, query: string, limit = 50): Promise<Message[]> {
    if (!wsManager.isConnected()) {
      return Promise.reject(new Error('Not connected to server'));
    }

    const trimmedQuery = query.trim();
    if (!trimmedQuery) {
      return Promise.resolve([]);
    }

    const boundedLimit = Math.min(Math.max(Math.floor(limit), 1), MAX_SEARCH_RESULTS);
    const requestId = requestIdCounter++;

    return new Promise<Message[]>((resolve, reject) => {
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
  function edit(messageId: number, text: string): void {
    if (!wsManager.isConnected()) return;
    if (typeof messageId !== 'number' || Number.isNaN(messageId)) return;

    const trimmed = text.trim();
    if (!trimmed) return;

    const payload = { type: 'edit-message', messageId, text };
    wsManager.send(payload);
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
    clearPendingSearches('Disconnected');
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

  return {
    subscribe,
    connect,
    connectionLost,
    send,
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
