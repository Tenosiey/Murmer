import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';

export type ChannelNotificationPreference = 'all' | 'mentions' | 'mute';

type ChannelNotificationState = Record<string, ChannelNotificationPreference>;

const STORAGE_KEY = 'murmer_channel_notifications';

function loadInitialState(): ChannelNotificationState {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as ChannelNotificationState;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: ChannelNotificationState = {};
    for (const [channel, value] of Object.entries(parsed)) {
      if (typeof channel !== 'string') continue;
      if (value === 'mentions' || value === 'mute') {
        result[channel] = value;
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
    const serializable: ChannelNotificationState = {};
    for (const [channel, value] of Object.entries(state)) {
      if (value !== 'all') {
        serializable[channel] = value;
      }
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(serializable));
  } catch (error) {
    console.error('Failed to persist channel notification settings', error);
  }
}

function normaliseChannel(channel: string): string | null {
  const trimmed = channel.trim();
  if (!trimmed) return null;
  return trimmed;
}

function createChannelNotificationsStore() {
  const store = writable<ChannelNotificationState>(loadInitialState());

  store.subscribe((value) => {
    persistState(value);
  });

  return {
    subscribe: store.subscribe,
    setPreference(channel: string, preference: ChannelNotificationPreference) {
      const key = normaliseChannel(channel);
      if (!key) return;
      store.update((current) => {
        const next = { ...current };
        if (preference === 'all') {
          delete next[key];
        } else {
          next[key] = preference;
        }
        return next;
      });
    },
    getPreference(channel: string): ChannelNotificationPreference {
      const key = normaliseChannel(channel);
      if (!key) return 'all';
      const state = get(store);
      return state[key] ?? 'all';
    },
    clear(channel: string) {
      const key = normaliseChannel(channel);
      if (!key) return;
      store.update((current) => {
        if (!(key in current)) return current;
        const next = { ...current };
        delete next[key];
        return next;
      });
    }
  };
}

export const channelNotifications = createChannelNotificationsStore();
