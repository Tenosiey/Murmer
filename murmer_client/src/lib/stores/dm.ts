import { derived, get, writable } from 'svelte/store';
import type { Message } from '../types';

/** peer username -> conversation messages, oldest first */
type Conversations = Record<string, Message[]>;

/** peer username -> number of DMs received while the conversation was closed */
type UnreadCounts = Record<string, number>;

function messageKey(msg: Message): number | string {
  return typeof msg.id === 'number' ? msg.id : `${msg.timestamp}-${msg.text}`;
}

function mergeMessages(existing: Message[], incoming: Message[]): Message[] {
  const byKey = new Map<number | string, Message>();
  for (const msg of existing) byKey.set(messageKey(msg), msg);
  for (const msg of incoming) byKey.set(messageKey(msg), msg);
  return [...byKey.values()].sort((a, b) => {
    if (typeof a.id === 'number' && typeof b.id === 'number') return a.id - b.id;
    return (a.timestamp ?? '').localeCompare(b.timestamp ?? '');
  });
}

function createDmStore() {
  const conversations = writable<Conversations>({});
  const unreadCounts = writable<UnreadCounts>({});
  const activePeer = writable<string | null>(null);

  return {
    conversations: { subscribe: conversations.subscribe },
    unread: { subscribe: unreadCounts.subscribe },
    activePeer: { subscribe: activePeer.subscribe },

    /** Open the conversation with a user and clear its unread counter. */
    open(peer: string) {
      activePeer.set(peer);
      unreadCounts.update((current) => {
        if (!(peer in current)) return current;
        const next = { ...current };
        delete next[peer];
        return next;
      });
    },

    close() {
      activePeer.set(null);
    },

    getActive(): string | null {
      return get(activePeer);
    },

    /** Record a DM frame from the server (sent or received). */
    receive(msg: Message, currentUser: string | null) {
      const from = typeof msg.from === 'string' ? msg.from : null;
      const to = typeof msg.to === 'string' ? msg.to : null;
      if (!from || !to || !currentUser) return;
      const peer = from === currentUser ? to : from;

      conversations.update((current) => ({
        ...current,
        [peer]: mergeMessages(current[peer] ?? [], [msg])
      }));

      if (from !== currentUser && get(activePeer) !== peer) {
        unreadCounts.update((current) => ({
          ...current,
          [peer]: (current[peer] ?? 0) + 1
        }));
      }
    },

    /** Merge a history snapshot for a conversation. */
    setHistory(peer: string, messages: Message[]) {
      conversations.update((current) => ({
        ...current,
        [peer]: mergeMessages(current[peer] ?? [], messages)
      }));
    },

    /** Drop all conversations, e.g. when leaving a server. */
    reset() {
      conversations.set({});
      unreadCounts.set({});
      activePeer.set(null);
    }
  };
}

export const dm = createDmStore();

/** Total number of unread direct messages across all conversations. */
export const dmUnreadTotal = derived(dm.unread, (counts) =>
  Object.values(counts).reduce((sum, count) => sum + count, 0)
);
