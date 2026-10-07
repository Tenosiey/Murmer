import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { parseReadMarkers } from './unread';

/** The store is a module-level singleton, so every test takes a fresh one. */
async function loadUnread() {
  vi.resetModules();
  return (await import('./unread')).unread;
}

describe('parseReadMarkers', () => {
  it('keeps well-formed entries and drops the rest one by one', () => {
    expect(
      parseReadMarkers({
        type: 'read-markers',
        channels: { '1': 10, '2': 0, '3': -4, x: 5, '4': 'nine', '5': 1.5 },
        dms: {
          alice: { lastRead: 7, unread: 2 },
          bob: { lastRead: 0, unread: 3 },
          eve: { lastRead: '7', unread: 1 },
          mallory: null
        }
      })
    ).toEqual({
      channels: { 1: 10 },
      dms: { alice: { lastRead: 7, unread: 2 }, bob: { lastRead: 0, unread: 3 } }
    });
  });

  it('survives a frame missing both maps', () => {
    expect(parseReadMarkers({ type: 'read-markers' })).toEqual({ channels: {}, dms: {} });
  });
});

describe('unread — last-read pointer', () => {
  it('only ever moves forward, and says when the server needs to hear', async () => {
    const unread = await loadUnread();

    expect(unread.markRead(1, 20)).toBe(true);
    expect(unread.markRead(1, 10)).toBe(false);
    expect(unread.markRead(1, 20)).toBe(false);
    expect(unread.getLastRead(1)).toBe(20);

    expect(unread.markRead(1, 21)).toBe(true);
    expect(unread.getLastRead(1)).toBe(21);
  });

  it('adopts the server markers without moving a newer local one back', async () => {
    const unread = await loadUnread();
    unread.markRead(1, 30);

    unread.load({ 1: 10, 2: 40 });
    expect(unread.getLastRead(1)).toBe(30);
    expect(unread.getLastRead(2)).toBe(40);
  });

  it('rejects non-positive and non-finite message ids', async () => {
    const unread = await loadUnread();

    for (const messageId of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      unread.markRead(1, messageId);
      expect(unread.getLastRead(1)).toBe(0);
    }
  });
});

describe('unread — counters', () => {
  it('counts incoming messages for inactive channels only', async () => {
    const unread = await loadUnread();
    unread.setActive(1);

    unread.recordIncoming(1, 100, false);
    unread.recordIncoming(2, 100, false);
    unread.recordIncoming(2, 101, true);

    expect(get(unread)).toEqual({ 2: { count: 2, mentions: 1 } });
  });

  it('ignores messages the user has already read', async () => {
    const unread = await loadUnread();
    unread.markRead(2, 50);

    unread.recordIncoming(2, 50, false);
    unread.recordIncoming(2, 49, false);
    expect(get(unread)).toEqual({});

    unread.recordIncoming(2, 51, false);
    expect(get(unread)).toEqual({ 2: { count: 1, mentions: 0 } });
  });

  it('clears a channel counter when it becomes active or is marked read', async () => {
    const unread = await loadUnread();
    unread.setActive(1);

    unread.recordIncoming(2, 10, true);
    unread.recordIncoming(3, 10, false);
    unread.setActive(2);
    expect(unread.getActive()).toBe(2);
    expect(get(unread)).toEqual({ 3: { count: 1, mentions: 0 } });

    unread.markRead(3, 10);
    expect(get(unread)).toEqual({});
  });

  it('drops counters, markers and the active channel on reset', async () => {
    const unread = await loadUnread();
    unread.setActive(1);
    unread.recordIncoming(2, 10, false);
    unread.markRead(3, 77);

    unread.reset();
    expect(get(unread)).toEqual({});
    expect(unread.getActive()).toBe(0);

    // Channel 1 is no longer active, so its messages count again.
    unread.recordIncoming(1, 11, false);
    expect(get(unread)).toEqual({ 1: { count: 1, mentions: 0 } });

    // The markers belong to the server just left; the next one sends its own.
    expect(unread.getLastRead(3)).toBe(0);
  });
});
