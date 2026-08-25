/**
 * The invite list is the dashboard's view of live credentials, and two things
 * about it are easy to get quietly wrong.
 *
 * The rows arrive off the wire, so a malformed one must be dropped rather than
 * rendered as an invite that does not exist — the tab offers a Revoke button
 * next to every row it shows.
 *
 * And `inviteSpent` is what tells an admin an invite has stopped admitting
 * anybody. It is the client's cosmetic mirror of the server's own check, so
 * the two must agree on the boundaries: a use limit is reached *at* the limit,
 * an expiry has passed *at* the timestamp.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { inviteSpent, type InviteEntry } from './invites';

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
  const [module, { connection }] = await Promise.all([
    import('./invites'),
    import('./connection')
  ]);
  connection.set('connected');
  return { ...module, connection };
}

beforeEach(() => {
  bus.reset();
});

describe('invites — the dashboard invite list', () => {
  it('is unknown until the server answers', async () => {
    const { invites } = await load();

    expect(get(invites)).toBeNull();
    invites.refresh();
    expect(bus.sent).toEqual([{ type: 'get-invites' }]);
  });

  it('parses the list and drops rows without a code', async () => {
    const { invites } = await load();

    bus.emit('invite-list', {
      invites: [
        {
          code: 'abc',
          createdBy: 'alice',
          createdAt: '2026-08-01T10:00:00Z',
          expiresAt: '2026-08-02T10:00:00Z',
          maxUses: 5,
          uses: 2
        },
        { createdBy: 'nobody' },
        { code: 'bare' }
      ]
    });

    expect(get(invites)).toEqual([
      {
        code: 'abc',
        createdBy: 'alice',
        createdAt: '2026-08-01T10:00:00Z',
        expiresAt: '2026-08-02T10:00:00Z',
        maxUses: 5,
        uses: 2
      },
      { code: 'bare', createdBy: '', createdAt: null, expiresAt: null, maxUses: 0, uses: 0 }
    ]);
  });

  it('ignores a frame whose invites field is not a list', async () => {
    const { invites } = await load();

    bus.emit('invite-list', { invites: [{ code: 'abc' }] });
    bus.emit('invite-list', { invites: 'all of them' });

    expect(get(invites)).toHaveLength(1);
  });

  it('sends the lifetime and use limit as numbers, never a date', async () => {
    // A date computed on the client would be wrong on a client whose clock
    // is; the server turns the lifetime into an expiry against its own.
    const { invites } = await load();

    invites.create(3600, 5);
    invites.revoke('abc');

    expect(bus.sent).toEqual([
      { type: 'create-invite', expiresIn: 3600, maxUses: 5 },
      { type: 'revoke-invite', code: 'abc' }
    ]);
  });

  it('forgets the list on disconnect so one server does not show another', async () => {
    const { invites, connection } = await load();

    bus.emit('invite-list', { invites: [{ code: 'abc' }] });
    expect(get(invites)).toHaveLength(1);

    connection.set('disconnected');
    expect(get(invites)).toBeNull();
  });
});

describe('inviteSpent', () => {
  const now = Date.parse('2026-08-01T12:00:00Z');

  /** A live invite, narrowed per test to the field under examination. */
  function invite(fields: Partial<InviteEntry>): InviteEntry {
    return {
      code: 'abc',
      createdBy: '',
      createdAt: null,
      expiresAt: null,
      maxUses: 0,
      uses: 0,
      ...fields
    };
  }

  it('never spends an invite with no limit and no expiry', () => {
    expect(inviteSpent(invite({ uses: 9999 }), now)).toBe(false);
  });

  it('spends one at its use limit, not before', () => {
    expect(inviteSpent(invite({ maxUses: 2, uses: 1 }), now)).toBe(false);
    expect(inviteSpent(invite({ maxUses: 2, uses: 2 }), now)).toBe(true);
    expect(inviteSpent(invite({ maxUses: 2, uses: 3 }), now)).toBe(true);
  });

  it('spends one at its expiry, not before', () => {
    expect(inviteSpent(invite({ expiresAt: '2026-08-01T12:00:01Z' }), now)).toBe(false);
    expect(inviteSpent(invite({ expiresAt: '2026-08-01T12:00:00Z' }), now)).toBe(true);
  });

  it('treats an undatable expiry as still live rather than hiding the row', () => {
    // The row is what carries the Revoke button, so a value the client cannot
    // read must not make the invite look already gone.
    expect(inviteSpent(invite({ expiresAt: 'sometime' }), now)).toBe(false);
  });
});
