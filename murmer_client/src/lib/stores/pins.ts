import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';
import type { Message } from '../types';

export interface PinnedEntry {
  id: number;
  user?: string;
  text?: string;
  image?: string;
  timestamp?: string;
  pinnedAt: string;
}

type PinnedState = Record<string, PinnedEntry[]>;

const STORAGE_KEY = 'murmer_pinned_messages';
const MAX_PINS_PER_CHANNEL = 25;

function loadInitialState(): PinnedState {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as PinnedState;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: PinnedState = {};
    for (const [channel, entries] of Object.entries(parsed)) {
      if (!Array.isArray(entries)) continue;
      const safeEntries = entries
        .filter((entry): entry is PinnedEntry => typeof entry === 'object' && entry !== null && typeof entry.id === 'number')
        .map((entry) => ({
          id: entry.id,
          user: typeof entry.user === 'string' ? entry.user : undefined,
          text: typeof entry.text === 'string' ? entry.text : undefined,
          image: typeof entry.image === 'string' ? entry.image : undefined,
          timestamp: typeof entry.timestamp === 'string' ? entry.timestamp : undefined,
          pinnedAt: typeof entry.pinnedAt === 'string' ? entry.pinnedAt : new Date().toISOString()
        }));
      if (safeEntries.length > 0) {
        result[channel] = safeEntries.slice(0, MAX_PINS_PER_CHANNEL);
      }
    }
    return result;
  } catch (error) {
    console.error('Failed to parse pinned messages from storage', error);
    return {};
  }
}

function persist(state: PinnedState) {
  if (!browser) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch (error) {
    console.error('Failed to persist pinned messages', error);
  }
}

function createSummary(message: Message): PinnedEntry {
  const text = typeof message.text === 'string' ? message.text : undefined;
  const image = typeof message.image === 'string' ? message.image : undefined;
  const timestamp = typeof message.timestamp === 'string' ? message.timestamp : undefined;
  return {
    id: message.id as number,
    user: typeof message.user === 'string' ? message.user : undefined,
    text,
    image,
    timestamp,
    pinnedAt: new Date().toISOString()
  };
}

function createPinnedStore() {
  const store = writable<PinnedState>(loadInitialState());

  store.subscribe((value) => {
    persist(value);
  });

  return {
    subscribe: store.subscribe,
    pin(channel: string, message: Message) {
      if (typeof message.id !== 'number') return;
      const trimmed = channel.trim();
      if (!trimmed) return;
      const summary = createSummary(message);
      store.update((current) => {
        const existing = current[trimmed] ?? [];
        const filtered = existing.filter((entry) => entry.id !== summary.id);
        const nextEntries = [summary, ...filtered].slice(0, MAX_PINS_PER_CHANNEL);
        return { ...current, [trimmed]: nextEntries };
      });
    },
    unpin(channel: string, messageId: number) {
      const trimmed = channel.trim();
      if (!trimmed) return;
      store.update((current) => {
        const existing = current[trimmed];
        if (!existing) return current;
        const nextEntries = existing.filter((entry) => entry.id !== messageId);
        if (nextEntries.length === 0) {
          const next = { ...current };
          delete next[trimmed];
          return next;
        }
        return { ...current, [trimmed]: nextEntries };
      });
    },
    removeMessage(channel: string, messageId: number) {
      const trimmed = channel.trim();
      if (!trimmed) return;
      store.update((current) => {
        const existing = current[trimmed];
        if (!existing) return current;
        const nextEntries = existing.filter((entry) => entry.id !== messageId);
        if (nextEntries.length === 0) {
          const next = { ...current };
          delete next[trimmed];
          return next;
        }
        return { ...current, [trimmed]: nextEntries };
      });
    },
    isPinned(channel: string, messageId: number): boolean {
      const trimmed = channel.trim();
      if (!trimmed) return false;
      const state = get(store);
      const entries = state[trimmed];
      if (!entries) return false;
      return entries.some((entry) => entry.id === messageId);
    }
  };
}

export const pinned = createPinnedStore();
