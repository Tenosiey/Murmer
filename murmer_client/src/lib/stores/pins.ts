import { get, writable } from 'svelte/store';

export interface PinnedEntry {
  id: number;
  user?: string;
  text?: string;
  image?: string;
  timestamp?: string;
  pinnedAt: string;
  pinnedBy?: string;
}

type PinnedState = Record<number, PinnedEntry[]>;

/**
 * Pins are persisted on the server. This store mirrors the `pins` snapshots
 * the server sends on channel join and after every pin/unpin; pin changes are
 * requested through the chat store rather than applied locally.
 */
function parseEntry(raw: unknown): PinnedEntry | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const entry = raw as Record<string, unknown>;
  if (typeof entry.id !== 'number') return null;
  return {
    id: entry.id,
    user: typeof entry.user === 'string' ? entry.user : undefined,
    text: typeof entry.text === 'string' ? entry.text : undefined,
    image: typeof entry.image === 'string' ? entry.image : undefined,
    timestamp: typeof entry.timestamp === 'string' ? entry.timestamp : undefined,
    pinnedAt: typeof entry.pinnedAt === 'string' ? entry.pinnedAt : new Date().toISOString(),
    pinnedBy: typeof entry.pinnedBy === 'string' ? entry.pinnedBy : undefined
  };
}

function createPinnedStore() {
  const store = writable<PinnedState>({});

  return {
    subscribe: store.subscribe,
    /** Replace a channel's pins with a server snapshot. */
    setChannelPins(channelId: number, entries: unknown[]) {
      const parsed = entries
        .map(parseEntry)
        .filter((entry): entry is PinnedEntry => entry !== null);
      store.update((current) => {
        if (parsed.length === 0) {
          if (!(channelId in current)) return current;
          const next = { ...current };
          delete next[channelId];
          return next;
        }
        return { ...current, [channelId]: parsed };
      });
    },
    /** Drop a deleted message's pin without waiting for a server snapshot. */
    removeMessage(channelId: number, messageId: number) {
      store.update((current) => {
        const existing = current[channelId];
        if (!existing) return current;
        const nextEntries = existing.filter((entry) => entry.id !== messageId);
        if (nextEntries.length === 0) {
          const next = { ...current };
          delete next[channelId];
          return next;
        }
        return { ...current, [channelId]: nextEntries };
      });
    },
    isPinned(channelId: number, messageId: number): boolean {
      const entries = get(store)[channelId];
      if (!entries) return false;
      return entries.some((entry) => entry.id === messageId);
    },
    /** Drop all pins, e.g. when leaving a server. */
    reset() {
      store.set({});
    }
  };
}

export const pinned = createPinnedStore();
