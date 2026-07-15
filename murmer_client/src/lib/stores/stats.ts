import { writable, get } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import { session } from './session';
import type { Message } from '../types';

/**
 * Lifetime user statistics with double opt-in privacy gating.
 *
 * The server only records stats when both the server-wide toggle (set by
 * Owners/Admins) and the user's own opt-in are enabled. This store mirrors
 * that configuration (`statsConfig`) and holds the most recently fetched
 * stats snapshot (`userStats`).
 */

export interface StatsConfig {
  /** Server-wide toggle controlled by Owners/Admins. */
  serverEnabled: boolean;
  /** The current user's own opt-in. */
  optedIn: boolean;
}

export interface FavoriteReaction {
  emoji: string;
  count: number;
}

export interface UserStats {
  messagesSent: number;
  messageChars: number;
  messageBytes: number;
  longestMessageChars: number;
  imagesSent: number;
  gifsSent: number;
  filesSent: number;
  uploadBytes: number;
  linksShared: number;
  repliesSent: number;
  mentionsSent: number;
  dmsSent: number;
  reactionsGiven: number;
  reactionsReceived: number;
  messagesEdited: number;
  messagesDeleted: number;
  pinsAdded: number;
  voiceSeconds: number;
  voiceSessions: number;
  screenshareSeconds: number;
}

export interface UserStatsSnapshot {
  user: string;
  trackedSince: string | null;
  stats: UserStats;
  favoriteReactions: FavoriteReaction[];
}

const EMPTY_STATS: UserStats = {
  messagesSent: 0,
  messageChars: 0,
  messageBytes: 0,
  longestMessageChars: 0,
  imagesSent: 0,
  gifsSent: 0,
  filesSent: 0,
  uploadBytes: 0,
  linksShared: 0,
  repliesSent: 0,
  mentionsSent: 0,
  dmsSent: 0,
  reactionsGiven: 0,
  reactionsReceived: 0,
  messagesEdited: 0,
  messagesDeleted: 0,
  pinsAdded: 0,
  voiceSeconds: 0,
  voiceSessions: 0,
  screenshareSeconds: 0
};

function toNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function parseSnapshot(msg: Message): UserStatsSnapshot | null {
  const payload = msg as any;
  if (typeof payload.user !== 'string') return null;
  const raw = payload.stats ?? {};
  const stats: UserStats = { ...EMPTY_STATS };
  for (const key of Object.keys(EMPTY_STATS) as (keyof UserStats)[]) {
    stats[key] = toNumber(raw[key]);
  }
  const favorites: FavoriteReaction[] = Array.isArray(payload.favoriteReactions)
    ? payload.favoriteReactions
        .filter((f: any) => f && typeof f.emoji === 'string')
        .map((f: any) => ({ emoji: f.emoji, count: toNumber(f.count) }))
    : [];
  return {
    user: payload.user,
    trackedSince: typeof payload.trackedSince === 'string' ? payload.trackedSince : null,
    stats,
    favoriteReactions: favorites
  };
}

function createStatsStores() {
  /** null until the server announced its configuration after auth. */
  const config = writable<StatsConfig | null>(null);
  /** The most recently received stats snapshot (own or another user's). */
  const snapshot = writable<UserStatsSnapshot | null>(null);

  chat.on('stats-config', (msg: Message) => {
    const payload = msg as any;
    config.update((current) => ({
      // A broadcast toggle change omits `optedIn`; keep the known value then.
      serverEnabled:
        typeof payload.serverEnabled === 'boolean'
          ? payload.serverEnabled
          : (current?.serverEnabled ?? false),
      optedIn:
        typeof payload.optedIn === 'boolean' ? payload.optedIn : (current?.optedIn ?? false)
    }));
  });

  chat.on('user-stats', (msg: Message) => {
    const parsed = parseSnapshot(msg);
    if (parsed) snapshot.set(parsed);
  });

  connection.subscribe((state) => {
    if (state !== 'connected') {
      // Forget the previous server's configuration and numbers.
      config.set(null);
      snapshot.set(null);
    }
  });

  /** Update the own opt-in; `purge` additionally deletes recorded counters. */
  function setOptIn(enabled: boolean, purge = false): void {
    chat.sendRaw({ type: 'set-stats-opt-in', enabled, purge });
  }

  /** Toggle the server-wide switch (Owner/Admin only; enforced server-side). */
  function setServerEnabled(enabled: boolean): void {
    chat.sendRaw({ type: 'set-stats-enabled', enabled });
  }

  /** Request a stats snapshot; omitting `user` fetches the own stats. */
  function fetchStats(user?: string): void {
    const target = user ?? get(session).user ?? undefined;
    chat.sendRaw({ type: 'get-user-stats', user: target });
  }

  /** Delete all own recorded counters (keeps the opt-in preference). */
  function resetStats(): void {
    chat.sendRaw({ type: 'reset-stats' });
  }

  return { config, snapshot, setOptIn, setServerEnabled, fetchStats, resetStats };
}

const stores = createStatsStores();

export const statsConfig = { subscribe: stores.config.subscribe };
export const statsSnapshot = { subscribe: stores.snapshot.subscribe };
export const stats = {
  setOptIn: stores.setOptIn,
  setServerEnabled: stores.setServerEnabled,
  fetchStats: stores.fetchStats,
  resetStats: stores.resetStats
};
