import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

const STORAGE_KEY = 'murmer_last_read';

/**
 * The store is a module-level singleton that reads `localStorage` once at
 * import time, so every test needs a fresh module instance.
 */
async function loadUnread() {
  vi.resetModules();
  return (await import('./unread')).unread;
}

function persistedState(): Record<string, Record<string, number>> {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw ? JSON.parse(raw) : {};
}

describe('unread — per-server namespacing', () => {
  it('keeps the last-read pointer of one server out of another', async () => {
    const unread = await loadUnread();

    unread.setServer('https://one.example');
    unread.markRead(7, 42);
    expect(unread.getLastRead(7)).toBe(42);

    // Same channel id, different server: ids are only unique per server.
    unread.setServer('https://two.example');
    expect(unread.getLastRead(7)).toBe(0);
    unread.markRead(7, 5);
    expect(unread.getLastRead(7)).toBe(5);

    unread.setServer('https://one.example');
    expect(unread.getLastRead(7)).toBe(42);
  });

  it('persists under a server-keyed map', async () => {
    const unread = await loadUnread();

    unread.setServer('https://one.example');
    unread.markRead(1, 10);
    unread.setServer('https://two.example');
    unread.markRead(1, 3);

    expect(persistedState()).toEqual({
      'https://one.example': { 1: 10 },
      'https://two.example': { 1: 3 }
    });
  });

  it('restores persisted state for the selected server on reload', async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ 'https://one.example': { 4: 99 }, 'https://two.example': { 4: 1 } })
    );
    const unread = await loadUnread();

    unread.setServer('https://two.example');
    expect(unread.getLastRead(4)).toBe(1);
    unread.setServer('https://one.example');
    expect(unread.getLastRead(4)).toBe(99);
  });

  it('drops entries left over from the old flat format', async () => {
    // Pre-namespacing the payload was `{channelId: messageId}`; those values
    // are numbers, not objects, and must not be read back as a server name.
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ 1: 12, 2: 30, 'https://one.example': { 1: 7 } })
    );
    const unread = await loadUnread();

    unread.setServer('https://one.example');
    expect(unread.getLastRead(1)).toBe(7);

    unread.setServer('1');
    expect(unread.getLastRead(1)).toBe(0);
  });

  it('ignores malformed channel entries and corrupt payloads', async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        'https://one.example': {
          1: 5,
          2: 0,
          3: -4,
          4: 'nope',
          5: null,
          notANumber: 8
        }
      })
    );
    let unread = await loadUnread();
    unread.setServer('https://one.example');
    expect(unread.getLastRead(1)).toBe(5);
    for (const channelId of [2, 3, 4, 5]) {
      expect(unread.getLastRead(channelId)).toBe(0);
    }

    localStorage.setItem(STORAGE_KEY, 'not json');
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    unread = await loadUnread();
    consoleError.mockRestore();
    unread.setServer('https://one.example');
    expect(unread.getLastRead(1)).toBe(0);
  });

  it('does not persist anything before a server is selected', async () => {
    const unread = await loadUnread();

    unread.markRead(1, 10);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();

    // Selecting a server replaces the state rather than adopting the stray one.
    unread.setServer('https://one.example');
    expect(unread.getLastRead(1)).toBe(0);
  });

  it('survives a failing localStorage write', async () => {
    const unread = await loadUnread();
    const setItem = vi.spyOn(localStorage, 'setItem').mockImplementation(() => {
      throw new Error('quota exceeded');
    });
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

    unread.setServer('https://one.example');
    expect(() => unread.markRead(1, 10)).not.toThrow();
    expect(unread.getLastRead(1)).toBe(10);

    setItem.mockRestore();
    consoleError.mockRestore();
  });
});

describe('unread — last-read pointer', () => {
  it('only ever moves forward', async () => {
    const unread = await loadUnread();
    unread.setServer('https://one.example');

    unread.markRead(1, 20);
    unread.markRead(1, 10);
    expect(unread.getLastRead(1)).toBe(20);

    unread.markRead(1, 21);
    expect(unread.getLastRead(1)).toBe(21);
  });

  it('rejects non-positive and non-finite message ids', async () => {
    const unread = await loadUnread();
    unread.setServer('https://one.example');

    for (const messageId of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      unread.markRead(1, messageId);
      expect(unread.getLastRead(1)).toBe(0);
    }
  });
});

describe('unread — counters', () => {
  it('counts incoming messages for inactive channels only', async () => {
    const unread = await loadUnread();
    unread.setServer('https://one.example');
    unread.setActive(1);

    unread.recordIncoming(1, 100, false);
    unread.recordIncoming(2, 100, false);
    unread.recordIncoming(2, 101, true);

    expect(get(unread)).toEqual({ 2: { count: 2, mentions: 1 } });
  });

  it('ignores messages the user has already read', async () => {
    const unread = await loadUnread();
    unread.setServer('https://one.example');
    unread.markRead(2, 50);

    unread.recordIncoming(2, 50, false);
    unread.recordIncoming(2, 49, false);
    expect(get(unread)).toEqual({});

    unread.recordIncoming(2, 51, false);
    expect(get(unread)).toEqual({ 2: { count: 1, mentions: 0 } });
  });

  it('clears a channel counter when it becomes active or is marked read', async () => {
    const unread = await loadUnread();
    unread.setServer('https://one.example');
    unread.setActive(1);

    unread.recordIncoming(2, 10, true);
    unread.recordIncoming(3, 10, false);
    unread.setActive(2);
    expect(unread.getActive()).toBe(2);
    expect(get(unread)).toEqual({ 3: { count: 1, mentions: 0 } });

    unread.markRead(3, 10);
    expect(get(unread)).toEqual({});
  });

  it('drops counters and the active channel on reset', async () => {
    const unread = await loadUnread();
    unread.setServer('https://one.example');
    unread.setActive(1);
    unread.recordIncoming(2, 10, false);
    unread.markRead(3, 77);

    unread.reset();
    expect(get(unread)).toEqual({});
    expect(unread.getActive()).toBe(0);

    // Channel 1 is no longer active, so its messages count again.
    unread.recordIncoming(1, 11, false);
    expect(get(unread)).toEqual({ 1: { count: 1, mentions: 0 } });

    // Last-read pointers are persistent and survive the reset.
    expect(unread.getLastRead(3)).toBe(77);
  });
});
