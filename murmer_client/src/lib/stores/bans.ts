/**
 * The server's ban list, shown in the Server Dashboard's Moderation tab.
 *
 * The rows carry each banned user's public key, so the server only answers
 * `get-ban-list` for moderators — this store is a cache of that answer, not a
 * permission check. It refreshes itself whenever a ban is lifted or a member
 * is banned, because both happen from elsewhere in the app (the member
 * context menu) while the dashboard may be open.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message } from '../types';

export interface BanEntry {
  /** Account name of the banned user. */
  user: string;
  /** Their Ed25519 public key, the identity the ban is really bound to. */
  publicKey: string;
  /** Who issued the ban; empty when it predates the record. */
  bannedBy: string;
  /** RFC 3339 timestamp, or null when the server sent something unparseable. */
  bannedAt: string | null;
}

function parseBan(entry: unknown): BanEntry | null {
  if (!entry || typeof entry !== 'object') return null;
  const row = entry as Record<string, unknown>;
  if (typeof row.user !== 'string' || !row.user) return null;
  return {
    user: row.user,
    publicKey: typeof row.publicKey === 'string' ? row.publicKey : '',
    bannedBy: typeof row.bannedBy === 'string' ? row.bannedBy : '',
    bannedAt: typeof row.bannedAt === 'string' ? row.bannedAt : null
  };
}

function createBansStore() {
  /** null until the server has answered a request on this connection. */
  const { subscribe, set } = writable<BanEntry[] | null>(null);

  chat.on('ban-list', (msg: Message) => {
    const raw = (msg as any).bans;
    if (!Array.isArray(raw)) return;
    set(raw.map(parseBan).filter((ban): ban is BanEntry => ban !== null));
  });

  // A ban or unban issued anywhere (member context menu, another moderator)
  // invalidates the cached list; the server is the one that knows the truth.
  // A kick uses the same frame as a ban and changes nothing here, so it is
  // told apart rather than costing a round trip.
  chat.on('user-unbanned', () => refresh());
  chat.on('force-disconnect', (msg: Message) => {
    if ((msg as any).action === 'banned') refresh();
  });

  connection.subscribe((state) => {
    if (state !== 'connected') set(null);
  });

  /** Ask the server for the current ban list (moderators only). */
  function refresh(): void {
    chat.sendRaw({ type: 'get-ban-list' });
  }

  /** Lift a ban by account name (requires Ban Members, enforced server-side). */
  function unban(user: string): void {
    chat.sendRaw({ type: 'unban-user', user });
  }

  return { subscribe, refresh, unban };
}

export const bans = createBansStore();
