import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';

const STORAGE_KEY = 'murmer_last_read';

/** channelId -> id of the newest message the user has seen */
type LastReadState = Record<number, number>;

/** serverUrl -> last-read state. Channel ids are only unique per server, so
 *  the persisted state must be namespaced or servers bleed into each other. */
type PersistedLastRead = Record<string, LastReadState>;

export interface UnreadInfo {
  count: number;
  mentions: number;
}

/** channelId -> unread counters (session-local; last-read ids persist) */
type UnreadCounts = Record<number, UnreadInfo>;

function parseChannelMap(value: unknown): LastReadState {
  if (!value || typeof value !== 'object') return {};
  const result: LastReadState = {};
  for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
    const channelId = Number(key);
    if (Number.isNaN(channelId)) continue;
    if (typeof entry === 'number' && Number.isFinite(entry) && entry > 0) {
      result[channelId] = entry;
    }
  }
  return result;
}

function loadPersisted(): PersistedLastRead {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: PersistedLastRead = {};
    for (const [server, channels] of Object.entries(parsed)) {
      // Entries whose value is not an object stem from the old flat
      // (un-namespaced) format and are dropped.
      if (channels && typeof channels === 'object') {
        result[server] = parseChannelMap(channels);
      }
    }
    return result;
  } catch (error) {
    console.error('Failed to parse last-read state', error);
    return {};
  }
}

function createUnreadStore() {
  const counts = writable<UnreadCounts>({});
  const lastRead = writable<LastReadState>({});
  let activeChannelId = 0;
  let activeServer: string | null = null;
  const persisted = loadPersisted();

  lastRead.subscribe((value) => {
    if (!browser || !activeServer) return;
    persisted[activeServer] = value;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(persisted));
    } catch (error) {
      console.error('Failed to persist last-read state', error);
    }
  });

  function clearCounts(channelId: number) {
    counts.update((current) => {
      if (!(channelId in current)) return current;
      const next = { ...current };
      delete next[channelId];
      return next;
    });
  }

  return {
    subscribe: counts.subscribe,
    /** Switch to the given server's persisted last-read state. Must be
     *  called when connecting, before any channel is joined. */
    setServer(url: string) {
      activeServer = url;
      lastRead.set(persisted[url] ?? {});
    },
    /** Mark a channel as the one currently on screen; its counter resets. */
    setActive(channelId: number) {
      activeChannelId = channelId;
      clearCounts(channelId);
    },
    getActive(): number {
      return activeChannelId;
    },
    /** Advance the last-read pointer for a channel and reset its counter. */
    markRead(channelId: number, messageId: number) {
      if (!Number.isFinite(messageId) || messageId <= 0) return;
      lastRead.update((current) => {
        if ((current[channelId] ?? 0) >= messageId) return current;
        return { ...current, [channelId]: messageId };
      });
      clearCounts(channelId);
    },
    getLastRead(channelId: number): number {
      return get(lastRead)[channelId] ?? 0;
    },
    /** Count a message that arrived in a channel the user is not viewing. */
    recordIncoming(channelId: number, messageId: number, mention: boolean) {
      if (channelId === activeChannelId) return;
      if (messageId <= this.getLastRead(channelId)) return;
      counts.update((current) => {
        const existing = current[channelId] ?? { count: 0, mentions: 0 };
        return {
          ...current,
          [channelId]: {
            count: existing.count + 1,
            mentions: existing.mentions + (mention ? 1 : 0)
          }
        };
      });
    },
    /** Drop all session counters, e.g. when leaving a server. */
    reset() {
      counts.set({});
      activeChannelId = 0;
    }
  };
}

export const unread = createUnreadStore();
