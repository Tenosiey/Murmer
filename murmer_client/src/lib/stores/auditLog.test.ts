/**
 * The audit log store parses an untrusted server frame into rows that are then
 * rendered as a record of who did what. Two failures here are invisible in the
 * UI and both matter more than usual for this feature:
 *
 * - a malformed row silently becoming a *plausible* entry — an audit log that
 *   invents a row is worse than one that drops it, so a row without an id or
 *   an action is dropped rather than defaulted;
 * - the list surviving a disconnect, which would show one server's moderation
 *   history while connected to another.
 *
 * The action-label table is guarded separately by `test/server-mirror.test.ts`.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { auditActionLabel } from '../chat/audit';

/** Stand-in for the chat store, as in the other store tests. */
const bus = vi.hoisted(() => {
  const sent: Record<string, any>[] = [];
  const handlers = new Map<string, ((msg: any) => void)[]>();
  return {
    sent,
    handlers,
    reset() {
      sent.length = 0;
      handlers.clear();
    },
    emit(type: string, payload: Record<string, unknown>) {
      for (const handler of handlers.get(type) ?? []) handler({ type, ...payload });
    }
  };
});

vi.mock('./chat', () => ({
  chat: {
    sendRaw(data: Record<string, any>) {
      bus.sent.push(data);
    },
    on(type: string, callback: (msg: any) => void) {
      const list = bus.handlers.get(type) ?? [];
      list.push(callback);
      bus.handlers.set(type, list);
    },
    off() {}
  }
}));

/** The store wires up its handler on import, so each test gets a fresh one. */
async function load() {
  bus.reset();
  vi.resetModules();
  const [{ auditLog }, { connection }] = await Promise.all([
    import('./auditLog'),
    import('./connection')
  ]);
  connection.set('connected');
  return { auditLog, connection };
}

beforeEach(() => {
  bus.reset();
});

describe('auditLog — the dashboard audit log', () => {
  it('is unknown until the server answers', async () => {
    const { auditLog } = await load();

    // null is deliberately not the same as an empty log: one says the server
    // has not answered, the other that nothing has been recorded.
    expect(get(auditLog)).toBeNull();
    auditLog.refresh();
    expect(bus.sent).toEqual([{ type: 'get-audit-log' }]);
  });

  it('parses entries and fills in the optional fields', async () => {
    const { auditLog } = await load();

    bus.emit('audit-log', {
      entries: [
        {
          id: 7,
          action: 'ban',
          actor: 'mod',
          target: 'spammer',
          detail: '',
          at: '2026-08-01T10:00:00Z'
        },
        { id: 6, action: 'server-reset' }
      ]
    });

    expect(get(auditLog)).toEqual([
      {
        id: 7,
        action: 'ban',
        actor: 'mod',
        target: 'spammer',
        detail: '',
        at: '2026-08-01T10:00:00Z'
      },
      { id: 6, action: 'server-reset', actor: '', target: '', detail: '', at: null }
    ]);
  });

  it('drops a row it cannot render honestly', async () => {
    const { auditLog } = await load();

    bus.emit('audit-log', {
      entries: [
        { action: 'ban', actor: 'mod' },
        { id: 4, actor: 'mod' },
        { id: 'five', action: 'kick' },
        { id: 3, action: '' },
        null,
        { id: 2, action: 'kick', actor: 'mod', target: 'raider', detail: '', at: null }
      ]
    });

    expect(get(auditLog)).toEqual([
      { id: 2, action: 'kick', actor: 'mod', target: 'raider', detail: '', at: null }
    ]);
  });

  it('ignores a frame whose entries are not a list', async () => {
    const { auditLog } = await load();

    bus.emit('audit-log', { entries: [{ id: 1, action: 'kick' }] });
    bus.emit('audit-log', { entries: 'nope' });

    expect(get(auditLog)).toHaveLength(1);
  });

  it('forgets the log when the connection drops', async () => {
    const { auditLog, connection } = await load();

    bus.emit('audit-log', { entries: [{ id: 1, action: 'kick' }] });
    connection.set('idle');

    expect(get(auditLog)).toBeNull();
  });
});

describe('auditActionLabel', () => {
  it('falls back to the wire name for an action it does not know', () => {
    // An unlabelled action is what an older client meeting a newer server
    // sees. Showing the raw name is the point: dropping the row would make
    // the log quietly incomplete, which is the one thing it must never be.
    expect(auditActionLabel('ban')).toBe('Banned a member');
    expect(auditActionLabel('something-new')).toBe('something-new');
  });
});
