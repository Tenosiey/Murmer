import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

async function loadDm() {
  vi.resetModules();
  return (await import('./dm')).dm;
}

const fromAlice = (id: number) => ({ type: 'dm', id, from: 'alice', to: 'me', text: 'hi' });

describe('dm — read state', () => {
  it('takes the unread counts the server reports, except for an open conversation', async () => {
    const dm = await loadDm();
    dm.open('bob');
    dm.loadReadState({ alice: { lastRead: 4, unread: 2 }, bob: { lastRead: 1, unread: 5 } });
    expect(get(dm.unread)).toEqual({ alice: 2 });
  });

  it('does not count a message another client already marked read', async () => {
    const dm = await loadDm();
    dm.loadReadState({ alice: { lastRead: 10, unread: 0 } });

    dm.receive(fromAlice(9), 'me');
    expect(get(dm.unread)).toEqual({});
    dm.receive(fromAlice(11), 'me');
    expect(get(dm.unread)).toEqual({ alice: 1 });
  });

  it('marks forward only, clearing the counter either way', async () => {
    const dm = await loadDm();
    dm.receive(fromAlice(5), 'me');
    dm.receive(fromAlice(3), 'me');
    expect(dm.latestId('alice')).toBe(5);

    expect(dm.markRead('alice', 5)).toBe(true);
    expect(get(dm.unread)).toEqual({});
    expect(dm.markRead('alice', 3)).toBe(false);
  });
});
