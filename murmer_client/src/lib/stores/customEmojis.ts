import { derived, writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import { EMOJI_NAME_RE, EMOJI_SHORTCODE_RE } from '../chat/constants';
import type { Message } from '../types';

export interface CustomEmoji {
  name: string;
  url: string;
  uploadedBy?: string;
  createdAt?: string;
}

/**
 * Custom server emojis, keyed by name. The server pushes the full list as an
 * `emoji-list` frame after authentication and re-broadcasts it whenever a
 * moderator adds or removes an emoji, so this store never has to request
 * anything. URLs stay relative (`/files/...`) and are resolved against the
 * selected server at render time.
 */
function createCustomEmojiStore() {
  const { subscribe, set } = writable<Record<string, CustomEmoji>>({});

  chat.on('emoji-list', (msg: Message) => {
    const raw = (msg as any).emojis;
    if (!Array.isArray(raw)) return;
    const emojis: Record<string, CustomEmoji> = {};
    for (const entry of raw) {
      if (!entry || typeof entry !== 'object') continue;
      const name = typeof entry.name === 'string' ? entry.name : '';
      const url = typeof entry.url === 'string' ? entry.url : '';
      // Validate server data before acting on it: only well-formed names and
      // relative upload URLs make it into client state.
      if (!EMOJI_NAME_RE.test(name) || !url.startsWith('/files/')) continue;
      emojis[name] = {
        name,
        url,
        uploadedBy: typeof entry.uploadedBy === 'string' ? entry.uploadedBy : undefined,
        createdAt: typeof entry.createdAt === 'string' ? entry.createdAt : undefined
      };
    }
    set(emojis);
  });

  connection.subscribe((state) => {
    // A fresh list arrives with the next successful auth.
    if (state !== 'connected') set({});
  });

  return { subscribe };
}

export const customEmojis = createCustomEmojiStore();

/** Sorted list form for rendering emoji grids. */
export const customEmojiList = derived(customEmojis, ($emojis) =>
  Object.values($emojis).sort((a, b) => a.name.localeCompare(b.name))
);

/**
 * Resolve a reaction key of the form `:name:` to the matching custom emoji,
 * or null when it is not a shortcode or the emoji no longer exists.
 */
export function shortcodeToEmoji(
  key: string,
  emojis: Record<string, CustomEmoji>
): CustomEmoji | null {
  const match = EMOJI_SHORTCODE_RE.exec(key);
  if (!match) return null;
  return emojis[match[1]] ?? null;
}
