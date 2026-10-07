import { get, writable } from 'svelte/store';
import type { Message } from '../types';
import type { DmReadState } from './unread';

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
  /** peer username -> id of the newest message read in that conversation */
  let lastRead: Record<string, number> = {};

  function clearUnread(peer: string) {
    unreadCounts.update((current) => {
      if (!(peer in current)) return current;
      const next = { ...current };
      delete next[peer];
      return next;
    });
  }

  return {
    conversations: { subscribe: conversations.subscribe },
    unread: { subscribe: unreadCounts.subscribe },
    activePeer: { subscribe: activePeer.subscribe },

    /** Open the conversation with a user and clear its unread counter. */
    open(peer: string) {
      activePeer.set(peer);
      clearUnread(peer);
    },

    /** Adopt the server's read state, sent at sign-in: where each
     *  conversation was last read and how many messages wait past it. */
    loadReadState(state: Record<string, DmReadState>) {
      const counts: UnreadCounts = {};
      for (const [peer, { lastRead: id, unread }] of Object.entries(state)) {
        lastRead[peer] = Math.max(lastRead[peer] ?? 0, id);
        if (unread > 0 && get(activePeer) !== peer) counts[peer] = unread;
      }
      unreadCounts.set(counts);
    },

    /** Advance the read pointer for a conversation and clear its counter.
     *  Returns whether the pointer moved, i.e. whether the server needs to
     *  hear about it. */
    markRead(peer: string, messageId: number): boolean {
      clearUnread(peer);
      if ((lastRead[peer] ?? 0) >= messageId) return false;
      lastRead[peer] = messageId;
      return true;
    },

    /** Id of the newest message held for a conversation, if any. */
    latestId(peer: string): number | null {
      let latest: number | null = null;
      for (const msg of get(conversations)[peer] ?? []) {
        if (typeof msg.id === 'number' && (latest === null || msg.id > latest)) latest = msg.id;
      }
      return latest;
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

      // A message another client of ours already marked read is not new.
      const fresh = typeof msg.id !== 'number' || msg.id > (lastRead[peer] ?? 0);
      if (from !== currentUser && get(activePeer) !== peer && fresh) {
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
      lastRead = {};
    }
  };
}

export const dm = createDmStore();
