import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';

export type ChannelNotificationPreference = 'all' | 'mentions' | 'mute';

type ChannelNotificationState = Record<number, ChannelNotificationPreference>;

/** serverUrl -> per-channel preferences. Channel ids are only unique per
 *  server, so the persisted state must be namespaced or servers bleed into
 *  each other. */
type PersistedState = Record<string, ChannelNotificationState>;

const STORAGE_KEY = 'murmer_channel_notifications';

function parseChannelMap(value: unknown): ChannelNotificationState {
  if (!value || typeof value !== 'object') return {};
  const result: ChannelNotificationState = {};
  for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
    const channelId = Number(key);
    if (Number.isNaN(channelId)) continue;
    if (entry === 'mentions' || entry === 'mute') {
      result[channelId] = entry;
    }
  }
  return result;
}

function loadPersisted(): PersistedState {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: PersistedState = {};
    for (const [server, channels] of Object.entries(parsed)) {
      // Entries whose value is not an object stem from the old flat
      // (un-namespaced) format and are dropped.
      if (channels && typeof channels === 'object') {
        result[server] = parseChannelMap(channels);
      }
    }
    return result;
  } catch (error) {
    console.error('Failed to parse channel notification settings', error);
    return {};
  }
}

function createChannelNotificationsStore() {
  const store = writable<ChannelNotificationState>({});
  let activeServer: string | null = null;
  const persisted = loadPersisted();

  store.subscribe((value) => {
    if (!browser || !activeServer) return;
    // 'all' is the default and is never written; setPreference deletes it.
    persisted[activeServer] = value;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(persisted));
    } catch (error) {
      console.error('Failed to persist channel notification settings', error);
    }
  });

  return {
    subscribe: store.subscribe,
    /** Switch to the given server's persisted preferences. Must be called
     *  when connecting, before any messages arrive. */
    setServer(url: string) {
      activeServer = url;
      store.set(persisted[url] ?? {});
    },
    setPreference(channelId: number, preference: ChannelNotificationPreference) {
      store.update((current) => {
        const next = { ...current };
        if (preference === 'all') {
          delete next[channelId];
        } else {
          next[channelId] = preference;
        }
        return next;
      });
    },
    getPreference(channelId: number): ChannelNotificationPreference {
      const state = get(store);
      return state[channelId] ?? 'all';
    },
    clear(channelId: number) {
      store.update((current) => {
        if (!(channelId in current)) return current;
        const next = { ...current };
        delete next[channelId];
        return next;
      });
    }
  };
}

export const channelNotifications = createChannelNotificationsStore();
