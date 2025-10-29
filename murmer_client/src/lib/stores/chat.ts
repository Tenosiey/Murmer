import { writable, get } from 'svelte/store';
import type { Message } from '../types';
import { session } from './session';
import { notify } from '../notify';
import { channelNotifications } from './channelNotifications';
import { prepareMessage, containsMention, normalizeReactions } from '../message-utils';
import { WebSocketManager } from '../websocket-manager';

/** Maximum number of search results to request from server */
const MAX_SEARCH_RESULTS = 200;
/** Timeout for search requests in milliseconds */
const SEARCH_TIMEOUT_MS = 5000;

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

        // Handle notifications
        if (!current || prepared.user !== current) {
          const channelName = prepared.channel ?? 'general';
          const preferences = get(channelNotifications);
          const preference = preferences[channelName] ?? 'all';
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
        update((m) => [...msgs, ...m]);
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
    wsManager.connect(
      url,
      handleMessage,
      onOpen,
      () => clearPendingSearches('Connection closed'),
      () => clearPendingSearches('WebSocket error')
    );
  }

  /**
   * Send a chat message.
   * @param user - Username
   * @param text - Message text
   */
  function send(user: string, text: string): void {
    if (!wsManager.isConnected()) return;

    const now = new Date();
    const payload = {
      type: 'chat',
      user,
      text,
      time: now.toLocaleTimeString(),
      timestamp: now.toISOString()
    };
    wsManager.send(payload);
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
   * @param channel - Channel name
   * @param before - Optional message ID to load messages before
   * @param limit - Number of messages to load (default: 50)
   */
  function loadHistory(channel: string, before?: number, limit = 50): void {
    sendRaw({ type: 'load-history', channel, before, limit });
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
   * @param channel - Channel to search
   * @param query - Search query
   * @param limit - Maximum results (default: 50, max: 200)
   * @returns Promise resolving to matching messages
   */
  function search(channel: string, query: string, limit = 50): Promise<Message[]> {
    if (!wsManager.isConnected()) {
      return Promise.reject(new Error('Not connected to server'));
    }

    const trimmedQuery = query.trim();
    if (!trimmedQuery) {
      return Promise.resolve([]);
    }

    const trimmedChannel = channel.trim() || 'general';
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
        channel: trimmedChannel,
        query: trimmedQuery,
        limit: boundedLimit,
        requestId
      };
      wsManager.send(payload);
    });
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
    clearPendingSearches('Disconnected');
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
    send,
    sendEphemeral,
    sendRaw,
    loadHistory,
    react,
    search,
    delete: deleteMessage,
    on,
    off,
    disconnect,
    clear: () => set([])
  };
}

export const chat = createChatStore();
