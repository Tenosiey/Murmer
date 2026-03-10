import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';

export type ChannelNotificationPreference = 'all' | 'mentions' | 'mute';

type ChannelNotificationState = Record<number, ChannelNotificationPreference>;

const STORAGE_KEY = 'murmer_channel_notifications';

function loadInitialState(): ChannelNotificationState {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, string>;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: ChannelNotificationState = {};
    for (const [key, value] of Object.entries(parsed)) {
      const channelId = Number(key);
      if (Number.isNaN(channelId)) continue;
      if (value === 'mentions' || value === 'mute') {
        result[channelId] = value;
      }
    }
    return result;
  } catch (error) {
    console.error('Failed to parse channel notification settings', error);
    return {};
  }
}

function persistState(state: ChannelNotificationState) {
  if (!browser) return;
  try {
    const serializable: Record<string, ChannelNotificationPreference> = {};
    for (const [key, value] of Object.entries(state)) {
      if (value !== 'all') {
        serializable[key] = value as ChannelNotificationPreference;
      }
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(serializable));
  } catch (error) {
    console.error('Failed to persist channel notification settings', error);
  }
}

function createChannelNotificationsStore() {
  const store = writable<ChannelNotificationState>(loadInitialState());

  store.subscribe((value) => {
    persistState(value);
  });

  return {
    subscribe: store.subscribe,
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
