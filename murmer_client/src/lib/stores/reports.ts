/**
 * The message report queue, shown in the Server Dashboard's Reports tab.
 *
 * The rows say who reported whom, so the server only answers `list-reports`
 * with the reports in channels the requester moderates — this store is a
 * cache of that answer, not a permission check.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message } from '../types';

export interface ReportEntry {
  id: number;
  messageId: number;
  channelId: number;
  /** Account name of the reported message's author. */
  author: string;
  reporter: string;
  reason: string;
  createdAt: string | null;
  /** The message has been deleted since it was reported. */
  deleted: boolean;
  /** The start of the message, or null when it is sealed or gone. */
  excerpt: string | null;
}

export function parseReport(entry: unknown): ReportEntry | null {
  if (!entry || typeof entry !== 'object') return null;
  const row = entry as Record<string, unknown>;
  if (
    typeof row.id !== 'number' ||
    typeof row.messageId !== 'number' ||
    typeof row.channelId !== 'number'
  ) {
    return null;
  }
  return {
    id: row.id,
    messageId: row.messageId,
    channelId: row.channelId,
    author: typeof row.author === 'string' ? row.author : '',
    reporter: typeof row.reporter === 'string' ? row.reporter : '',
    reason: typeof row.reason === 'string' ? row.reason : '',
    createdAt: typeof row.createdAt === 'string' ? row.createdAt : null,
    deleted: row.deleted === true,
    excerpt: typeof row.excerpt === 'string' ? row.excerpt : null
  };
}

function createReportsStore() {
  /** null until the server has answered a request on this connection. */
  const { subscribe, set } = writable<ReportEntry[] | null>(null);

  chat.on('reports', (msg: Message) => {
    const raw = msg.reports;
    if (!Array.isArray(raw)) return;
    set(raw.map(parseReport).filter((report): report is ReportEntry => report !== null));
  });

  connection.subscribe((state) => {
    if (state !== 'connected') set(null);
  });

  /** Ask the server for the open reports this user may work. */
  function refresh(): void {
    chat.sendRaw({ type: 'list-reports' });
  }

  /** Close a report; the server answers with the queue as it now stands. */
  function resolve(id: number): void {
    chat.sendRaw({ type: 'resolve-report', id });
  }

  return { subscribe, refresh, resolve };
}

export const reports = createReportsStore();
