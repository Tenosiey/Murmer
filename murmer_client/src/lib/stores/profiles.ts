import { derived, readable, writable } from 'svelte/store';
import { chat } from './chat';
import {
  MAX_ABOUT_LENGTH,
  MAX_DISPLAY_NAME_LENGTH,
  MAX_NICKNAME_LENGTH,
  MAX_STATUS_TEXT_LENGTH
} from '../chat/constants';
import type { Message, UserProfile } from '../types';

/**
 * Per-user profiles (display name, nickname, about text, member since), keyed
 * by the account name. The server sends a `profile-snapshot` after
 * authentication — including offline users, so a profile can be opened for
 * anyone in the member list — and broadcasts `profile-update` on every change.
 *
 * The account name remains the identity: nothing here is ever used to address
 * a user. Display names and nicknames are cosmetic and may collide, which is
 * why every profile view also shows the account name.
 */

/** Validate a server-sent profile before it enters client state. */
function toProfile(raw: unknown): UserProfile | null {
  if (!raw || typeof raw !== 'object') return null;
  const p = raw as Record<string, unknown>;
  if (typeof p.user !== 'string' || !p.user) return null;
  const displayName = typeof p.displayName === 'string' ? p.displayName : '';
  const nickname = typeof p.nickname === 'string' ? p.nickname : '';
  const about = typeof p.about === 'string' ? p.about : '';
  return {
    user: p.user,
    // Trim to the same limits the server enforces so a tampered frame cannot
    // stretch a name across the member list.
    displayName: displayName.slice(0, MAX_DISPLAY_NAME_LENGTH),
    nickname: nickname.slice(0, MAX_NICKNAME_LENGTH),
    about: about.slice(0, MAX_ABOUT_LENGTH),
    statusText: typeof p.statusText === 'string' ? p.statusText.slice(0, MAX_STATUS_TEXT_LENGTH) : '',
    statusExpiresAt:
      typeof p.statusExpiresAt === 'number' && Number.isFinite(p.statusExpiresAt)
        ? p.statusExpiresAt
        : null,
    createdAt: typeof p.createdAt === 'string' ? p.createdAt : ''
  };
}

function createProfileStore() {
  const { subscribe, set, update } = writable<Record<string, UserProfile>>({});

  chat.on('profile-snapshot', (msg: Message) => {
    const list = msg.profiles;
    if (!Array.isArray(list)) return;
    const map: Record<string, UserProfile> = {};
    for (const entry of list) {
      const profile = toProfile(entry);
      if (profile) map[profile.user] = profile;
    }
    set(map);
  });

  chat.on('profile-update', (msg: Message) => {
    const profile = toProfile(msg.profile);
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

  /**
   * Set (or with an empty string clear) a member's nickname on this server.
   * Own nicknames are always allowed; somebody else's needs `MANAGE_NICKNAMES`
   * and outranking them, which only the server decides — the caller gets an
   * `error` frame when it does not. The change is confirmed by the broadcast.
   */
  function setNickname(user: string, nickname: string) {
    chat.sendRaw({ type: 'set-nickname', user, nickname });
  }

  /**
   * Set (or with an empty string clear) the own custom status line, lapsing
   * at `expiresAt` (Unix milliseconds) or never with null.
   */
  function setStatusText(text: string, expiresAt: number | null) {
    chat.sendRaw({ type: 'set-status-text', text, expiresAt });
  }

  return { subscribe, saveSelf, setNickname, setStatusText, reset: () => set({}) };
}

export const profiles = createProfileStore();

/**
 * What to show for a user: this server's nickname for them, else the display
 * name they chose, else their account name. Use this everywhere a name is
 * rendered.
 *
 * The nickname wins because it is the server's label — a moderator may have
 * set it, and a user must not be able to shrug that off by editing their own
 * display name.
 */
export const displayNames = derived(profiles, ($profiles) => {
  const map: Record<string, string> = {};
  for (const [user, profile] of Object.entries($profiles)) {
    const name = profile.nickname || profile.displayName;
    if (name) map[user] = name;
  }
  return (user: string) => map[user] ?? user;
});

/**
 * A profile's status line as of `now`, or '' once it has lapsed. The server
 * leaves a lapsed line out of what it sends, but nothing tells a connected
 * client when one runs out — so the client checks against its own clock.
 */
export function activeStatusText(profile: UserProfile | undefined, now: number): string {
  if (!profile?.statusText) return '';
  if (profile.statusExpiresAt !== null && profile.statusExpiresAt <= now) return '';
  return profile.statusText;
}

/** The clock status lines lapse against, ticking every thirty seconds. */
const statusClock = readable(Date.now(), (set) => {
  const timer = setInterval(() => set(Date.now()), 30_000);
  return () => clearInterval(timer);
});

/** Each user's current status line, '' when they have none. */
export const statusTexts = derived([profiles, statusClock], ([$profiles, now]) => {
  return (user: string) => activeStatusText($profiles[user], now);
});
