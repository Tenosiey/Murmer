import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';

const STORAGE_KEY = 'murmer_last_read';

/** channelId -> id of the newest message the user has seen */
type LastReadState = Record<number, number>;

export interface UnreadInfo {
  count: number;
  mentions: number;
}

/** channelId -> unread counters (session-local; last-read ids persist) */
type UnreadCounts = Record<number, UnreadInfo>;

function loadLastRead(): LastReadState {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: LastReadState = {};
    for (const [key, value] of Object.entries(parsed)) {
      const channelId = Number(key);
      if (Number.isNaN(channelId)) continue;
      if (typeof value === 'number' && Number.isFinite(value) && value > 0) {
        result[channelId] = value;
      }
    }
    return result;
  } catch (error) {
    console.error('Failed to parse last-read state', error);
    return {};
  }
}

function persistLastRead(state: LastReadState) {
  if (!browser) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch (error) {
    console.error('Failed to persist last-read state', error);
  }
}

function createUnreadStore() {
  const counts = writable<UnreadCounts>({});
  const lastRead = writable<LastReadState>(loadLastRead());
  let activeChannelId = 0;

  lastRead.subscribe((value) => {
    persistLastRead(value);
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
