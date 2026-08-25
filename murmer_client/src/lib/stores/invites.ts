/**
 * The server's invite codes, shown in the Server Dashboard's Invites tab.
 *
 * The rows *are* the credentials, so the server only answers `get-invites`
 * for members holding `CREATE_INVITES`; this store is a cache of that answer,
 * not a permission check. `null` means "not disclosed yet" and is deliberately
 * not the same as an empty list — an empty list is the server saying there are
 * no invites, which the tab renders differently from "still asking".
 *
 * A create or revoke is answered with the whole refreshed list, so nothing
 * here writes optimistically: the list only ever changes when the server says
 * it did.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message } from '../types';

export interface InviteEntry {
  /** The code itself — this is the secret the link carries. */
  code: string;
  /** Account name of whoever minted it; empty when unrecorded. */
  createdBy: string;
  /** RFC 3339 timestamp, or null when the server sent something unparseable. */
  createdAt: string | null;
  /** RFC 3339 expiry, or null when the invite never expires. */
  expiresAt: string | null;
  /** How many times it may be redeemed; 0 means unlimited. */
  maxUses: number;
  /** How many times it already has been. */
  uses: number;
}

function parseInvite(entry: unknown): InviteEntry | null {
  if (!entry || typeof entry !== 'object') return null;
  const row = entry as Record<string, unknown>;
  if (typeof row.code !== 'string' || !row.code) return null;
  return {
    code: row.code,
    createdBy: typeof row.createdBy === 'string' ? row.createdBy : '',
    createdAt: typeof row.createdAt === 'string' ? row.createdAt : null,
    expiresAt: typeof row.expiresAt === 'string' ? row.expiresAt : null,
    maxUses: typeof row.maxUses === 'number' && row.maxUses > 0 ? Math.floor(row.maxUses) : 0,
    uses: typeof row.uses === 'number' && row.uses > 0 ? Math.floor(row.uses) : 0
  };
}

/** Whether an invite can still admit somebody, for the row's status label. */
export function inviteSpent(invite: InviteEntry, now: number = Date.now()): boolean {
  if (invite.maxUses > 0 && invite.uses >= invite.maxUses) return true;
  if (!invite.expiresAt) return false;
  const expiry = Date.parse(invite.expiresAt);
  return Number.isFinite(expiry) && expiry <= now;
}

function createInvitesStore() {
  const { subscribe, set } = writable<InviteEntry[] | null>(null);

  chat.on('invite-list', (msg: Message) => {
    const raw = (msg as { invites?: unknown }).invites;
    if (!Array.isArray(raw)) return;
    set(raw.map(parseInvite).filter((invite): invite is InviteEntry => invite !== null));
  });

  connection.subscribe((state) => {
    if (state !== 'connected') set(null);
  });

  /** Ask the server for its invite list (requires Create Invites). */
  function refresh(): void {
    chat.sendRaw({ type: 'get-invites' });
  }

  /**
   * Mint a new invite. `expiresIn` is a lifetime in seconds and `maxUses` a
   * use limit; 0 means "no limit" for either. A lifetime rather than a date
   * so a wrong client clock cannot mint an already-expired invite.
   */
  function create(expiresIn: number, maxUses: number): void {
    chat.sendRaw({ type: 'create-invite', expiresIn, maxUses });
  }

  /** Withdraw an invite. Members who already joined through it stay. */
  function revoke(code: string): void {
    chat.sendRaw({ type: 'revoke-invite', code });
  }

  return { subscribe, refresh, create, revoke };
}

export const invites = createInvitesStore();
