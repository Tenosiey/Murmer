import { get, writable } from 'svelte/store';

/** channelId -> id of the newest message the user has seen */
type LastReadState = Record<number, number>;

export interface UnreadInfo {
  count: number;
  mentions: number;
}

/** channelId -> unread counters (session-local; last-read ids live on the server) */
type UnreadCounts = Record<number, UnreadInfo>;

/** Where the user stands in one direct conversation, as the server counts it. */
export interface DmReadState {
  lastRead: number;
  unread: number;
}

export interface ReadMarkers {
  channels: LastReadState;
  dms: Record<string, DmReadState>;
}

function isCount(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

/**
 * Parse the server's `read-markers` frame. Malformed entries are dropped one
 * by one rather than failing the frame: a marker that is missing only costs
 * the "new messages" divider for that channel.
 */
export function parseReadMarkers(msg: Record<string, unknown>): ReadMarkers {
  const result: ReadMarkers = { channels: {}, dms: {} };
  if (msg.channels && typeof msg.channels === 'object') {
    for (const [key, value] of Object.entries(msg.channels as Record<string, unknown>)) {
      const channelId = Number(key);
      if (Number.isSafeInteger(channelId) && isCount(value) && value > 0) {
        result.channels[channelId] = value;
      }
    }
  }
  if (msg.dms && typeof msg.dms === 'object') {
    for (const [peer, value] of Object.entries(msg.dms as Record<string, unknown>)) {
      if (!value || typeof value !== 'object') continue;
      const { lastRead, unread } = value as Record<string, unknown>;
      if (isCount(lastRead) && isCount(unread)) result.dms[peer] = { lastRead, unread };
    }
  }
  return result;
}

function createUnreadStore() {
  const counts = writable<UnreadCounts>({});
  let lastRead: LastReadState = {};
  let activeChannelId = 0;

  // Checks before calling `set`: a writable treats any object it is handed as
  // changed, so writing the same one back would still notify every badge.
  function clearCounts(channelId: number) {
    const current = get(counts);
    if (!(channelId in current)) return;
    const next = { ...current };
    delete next[channelId];
    counts.set(next);
  }

  return {
    subscribe: counts.subscribe,
    /** Adopt the server's markers, sent at sign-in. Only ever moves a
     *  pointer forward, in case this client already read further. */
    load(markers: LastReadState) {
      for (const [key, messageId] of Object.entries(markers)) {
        const channelId = Number(key);
        if ((lastRead[channelId] ?? 0) < messageId) lastRead[channelId] = messageId;
      }
    },
    /** Mark a channel as the one currently on screen; its counter resets. */
    setActive(channelId: number) {
      activeChannelId = channelId;
      clearCounts(channelId);
    },
    getActive(): number {
      return activeChannelId;
    },
    /** Advance the last-read pointer for a channel and reset its counter.
     *  Returns whether the pointer moved, i.e. whether the server needs
     *  to hear about it. */
    markRead(channelId: number, messageId: number): boolean {
      if (!Number.isFinite(messageId) || messageId <= 0) return false;
      clearCounts(channelId);
      if ((lastRead[channelId] ?? 0) >= messageId) return false;
      lastRead[channelId] = messageId;
      return true;
    },
    getLastRead(channelId: number): number {
      return lastRead[channelId] ?? 0;
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
    /** Drop everything held for the server, e.g. when leaving it. The
     *  markers come back with the next sign-in. */
    reset() {
      counts.set({});
      lastRead = {};
      activeChannelId = 0;
    }
  };
}

export const unread = createUnreadStore();
