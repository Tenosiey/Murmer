import { writable } from 'svelte/store';

/** How long a typing signal stays visible without being refreshed. */
const TYPING_EXPIRY_MS = 4000;

/** channelId -> user -> expiry timestamp (ms since epoch) */
type TypingState = Record<number, Record<string, number>>;

function createTypingStore() {
  const store = writable<TypingState>({});
  let pruneTimer: ReturnType<typeof setTimeout> | null = null;

  function prune() {
    pruneTimer = null;
    const now = Date.now();
    let nextExpiry = Infinity;
    store.update((state) => {
      const next: TypingState = {};
      for (const [channelKey, users] of Object.entries(state)) {
        const remaining: Record<string, number> = {};
        for (const [user, expiry] of Object.entries(users)) {
          if (expiry > now) {
            remaining[user] = expiry;
            nextExpiry = Math.min(nextExpiry, expiry);
          }
        }
        if (Object.keys(remaining).length > 0) {
          next[Number(channelKey)] = remaining;
        }
      }
      return next;
    });
    if (nextExpiry !== Infinity) {
      schedulePrune(nextExpiry - now);
    }
  }

  function schedulePrune(delay: number) {
    if (pruneTimer !== null) return;
    pruneTimer = setTimeout(prune, Math.max(delay, 250));
  }

  return {
    subscribe: store.subscribe,
    /** Record that a user is typing in a channel. */
    bump(channelId: number, user: string) {
      store.update((state) => ({
        ...state,
        [channelId]: { ...state[channelId], [user]: Date.now() + TYPING_EXPIRY_MS }
      }));
      schedulePrune(TYPING_EXPIRY_MS);
    },
    /** Drop a user's typing signal, e.g. once their message arrives. */
    clear(channelId: number, user: string) {
      store.update((state) => {
        const users = state[channelId];
        if (!users || !(user in users)) return state;
        const remaining = { ...users };
        delete remaining[user];
        const next = { ...state };
        if (Object.keys(remaining).length > 0) {
          next[channelId] = remaining;
        } else {
          delete next[channelId];
        }
        return next;
      });
    },
    reset() {
      store.set({});
    }
  };
}

export const typing = createTypingStore();
