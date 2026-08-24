/**
 * The server's audit log, shown in the Server Dashboard's Audit Log tab.
 *
 * Like the ban list this is a cache of an *answer*, not a broadcast: the rows
 * name who moderated whom, so the server only replies to `get-audit-log` for
 * a viewer holding `VIEW_AUDIT_LOG`. `null` means "not disclosed yet" and is
 * deliberately distinct from an empty log — one says the server has not
 * answered, the other that nothing has happened.
 *
 * Nothing here refreshes itself on a moderation event. The log is a record
 * rather than live state, and a moderator watching the tab while somebody
 * else works would otherwise cost a round trip per action; the tab re-asks
 * when it is opened, and offers a refresh button.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message } from '../types';

export interface AuditEntry {
  /** Row id, unique and increasing — used to key the list. */
  id: number;
  /** Wire name of the action; see `chat/audit.ts` for the labels. */
  action: string;
  /** Account name of whoever acted, or the `/role` endpoint's sentinel. */
  actor: string;
  /** What was acted on, or empty for a server-wide action. */
  target: string;
  /** Short summary of the change; may be empty. */
  detail: string;
  /** RFC 3339 timestamp, or null when the server sent something unparseable. */
  at: string | null;
}

function parseEntry(entry: unknown): AuditEntry | null {
  if (!entry || typeof entry !== 'object') return null;
  const row = entry as Record<string, unknown>;
  // The id keys the list and the action decides the label; a row missing
  // either is not something that can be rendered honestly, so it is dropped
  // rather than shown with a made-up value.
  if (typeof row.id !== 'number' || !Number.isFinite(row.id)) return null;
  if (typeof row.action !== 'string' || !row.action) return null;
  return {
    id: row.id,
    action: row.action,
    actor: typeof row.actor === 'string' ? row.actor : '',
    target: typeof row.target === 'string' ? row.target : '',
    detail: typeof row.detail === 'string' ? row.detail : '',
    at: typeof row.at === 'string' ? row.at : null
  };
}

function createAuditLogStore() {
  /** null until the server has answered a request on this connection. */
  const { subscribe, set } = writable<AuditEntry[] | null>(null);

  chat.on('audit-log', (msg: Message) => {
    const raw = (msg as { entries?: unknown }).entries;
    if (!Array.isArray(raw)) return;
    set(raw.map(parseEntry).filter((entry): entry is AuditEntry => entry !== null));
  });

  connection.subscribe((state) => {
    if (state !== 'connected') set(null);
  });

  /** Ask the server for the newest entries (`VIEW_AUDIT_LOG` only). */
  function refresh(): void {
    chat.sendRaw({ type: 'get-audit-log' });
  }

  return { subscribe, refresh };
}

export const auditLog = createAuditLogStore();
