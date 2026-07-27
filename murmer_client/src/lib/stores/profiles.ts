import { derived, writable } from 'svelte/store';
import { chat } from './chat';
import { MAX_ABOUT_LENGTH, MAX_DISPLAY_NAME_LENGTH } from '../chat/constants';
import type { Message, UserProfile } from '../types';

/**
 * Per-user profiles (display name, about text, member since), keyed by the
 * account name. The server sends a `profile-snapshot` after authentication —
 * including offline users, so a profile can be opened for anyone in the member
 * list — and broadcasts `profile-update` on every change.
 *
 * The account name remains the identity: nothing here is ever used to address
 * a user. Display names are cosmetic and may collide, which is why every
 * profile view also shows the account name.
 */

/** Validate a server-sent profile before it enters client state. */
function toProfile(raw: unknown): UserProfile | null {
  if (!raw || typeof raw !== 'object') return null;
  const p = raw as Record<string, unknown>;
  if (typeof p.user !== 'string' || !p.user) return null;
  const displayName = typeof p.displayName === 'string' ? p.displayName : '';
  const about = typeof p.about === 'string' ? p.about : '';
  return {
    user: p.user,
    // Trim to the same limits the server enforces so a tampered frame cannot
    // stretch a name across the member list.
    displayName: displayName.slice(0, MAX_DISPLAY_NAME_LENGTH),
    about: about.slice(0, MAX_ABOUT_LENGTH),
    createdAt: typeof p.createdAt === 'string' ? p.createdAt : ''
  };
}

function createProfileStore() {
  const { subscribe, set, update } = writable<Record<string, UserProfile>>({});

  chat.on('profile-snapshot', (msg: Message) => {
    const list = (msg as any).profiles;
    if (!Array.isArray(list)) return;
    const map: Record<string, UserProfile> = {};
    for (const entry of list) {
      const profile = toProfile(entry);
      if (profile) map[profile.user] = profile;
    }
    set(map);
  });

  chat.on('profile-update', (msg: Message) => {
    const profile = toProfile((msg as any).profile);
    if (!profile) return;
    update((map) => ({ ...map, [profile.user]: profile }));
  });

  /**
   * Update the own profile. Omitted fields are left untouched server-side;
   * an empty string clears one. The change is confirmed by the broadcast.
   */
  function saveSelf(fields: { displayName?: string; about?: string }) {
    chat.sendRaw({ type: 'set-profile', ...fields });
  }

  return { subscribe, saveSelf, reset: () => set({}) };
}

export const profiles = createProfileStore();

/**
 * What to show for a user: their display name when they set one, otherwise
 * their account name. Use this everywhere a name is rendered.
 */
export const displayNames = derived(profiles, ($profiles) => {
  const map: Record<string, string> = {};
  for (const [user, profile] of Object.entries($profiles)) {
    if (profile.displayName) map[user] = profile.displayName;
  }
  return (user: string) => map[user] ?? user;
});
