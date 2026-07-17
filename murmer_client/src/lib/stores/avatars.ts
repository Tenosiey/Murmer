import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

/**
 * Per-user avatars, keyed by user name. Values are `/files/<key>` upload
 * URLs relative to the connected server's HTTP base. The server sends an
 * `avatar-snapshot` after authentication and broadcasts `avatar-update`
 * frames on change; setting the own avatar is confirmed by that broadcast.
 */

/** Only accept upload paths; anything else cannot be a valid avatar and
    must not end up concatenated onto the server's HTTP base URL. */
function isUploadUrl(value: unknown): value is string {
  return typeof value === 'string' && value.startsWith('/files/');
}

function createAvatarStore() {
  const { subscribe, set, update } = writable<Record<string, string>>({});

  chat.on('avatar-snapshot', (msg: Message) => {
    const raw = (msg as any).avatars;
    if (!raw || typeof raw !== 'object') return;
    const normalized: Record<string, string> = {};
    for (const [user, url] of Object.entries(raw as Record<string, unknown>)) {
      if (isUploadUrl(url)) normalized[user] = url;
    }
    set(normalized);
  });

  chat.on('avatar-update', (msg: Message) => {
    const raw = msg as any;
    const user = typeof raw.user === 'string' ? raw.user : null;
    if (!user) return;
    update((map) => {
      if (isUploadUrl(raw.avatar)) return { ...map, [user]: raw.avatar };
      const { [user]: _removed, ...rest } = map;
      return rest;
    });
  });

  /** Set the own avatar to an uploaded image URL, or remove it with null. */
  function setSelf(url: string | null) {
    chat.sendRaw({ type: 'set-avatar', avatar: url });
  }

  return { subscribe, setSelf };
}

export const avatars = createAvatarStore();
