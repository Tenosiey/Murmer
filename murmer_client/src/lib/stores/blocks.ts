import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';
import { dialogs } from './dialogs';
import { t } from '../i18n';

/**
 * Blocked members: personal and local, like the per-user soundboard mute.
 * The server never learns who you blocked, so blocking hides rather than
 * prevents — their messages collapse behind a "show" button, their voice and
 * soundboard clips play silent and their DMs are dropped on arrival, but they
 * can still see and message you.
 *
 * Keyed by account name and namespaced by server URL: account names are only
 * unique per server, so the same name elsewhere is somebody else.
 */

const STORAGE_KEY = 'murmer_blocked_users';

type PersistedState = Record<string, string[]>;

function loadPersisted(): PersistedState {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: PersistedState = {};
    for (const [server, users] of Object.entries(parsed as Record<string, unknown>)) {
      if (!Array.isArray(users)) continue;
      result[server] = users.filter((u): u is string => typeof u === 'string' && !!u);
    }
    return result;
  } catch (error) {
    console.error('Failed to parse blocked users', error);
    return {};
  }
}

function createBlocksStore() {
  const store = writable<string[]>([]);
  let activeServer: string | null = null;
  const persisted = loadPersisted();

  store.subscribe((value) => {
    if (!browser || !activeServer) return;
    persisted[activeServer] = value;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(persisted));
    } catch (error) {
      console.error('Failed to persist blocked users', error);
    }
  });

  return {
    subscribe: store.subscribe,
    /** Switch to the given server's list. Called on connect. */
    setServer(url: string) {
      activeServer = url;
      store.set(persisted[url] ?? []);
    },
    setBlocked(user: string, blocked: boolean) {
      store.update((current) => {
        if (current.includes(user) === blocked) return current;
        return blocked ? [...current, user] : current.filter((u) => u !== user);
      });
    }
  };
}

export const blockedUsers = createBlocksStore();

/** Ask, then block `user`; `name` is what the dialog calls them. */
export async function confirmBlock(user: string, name: string): Promise<void> {
  const confirmed = await dialogs.confirm({
    title: t('block.title', { name }),
    message: t('block.message'),
    confirmLabel: t('block.confirm'),
    danger: true
  });
  if (confirmed) blockedUsers.setBlocked(user, true);
}

/** Whether `user` is blocked on the current server. */
export function isBlocked(user: string | null | undefined): boolean {
  return !!user && get(blockedUsers).includes(user);
}
