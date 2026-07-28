import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import type { Message } from '../types';

/** Server cap the store chunks `wiki-resolve` requests to. */
const MAX_RESOLVE_LINKS = 50;
const REQUEST_TIMEOUT_MS = 5000;

/**
 * Stand-in for the chat store: records outgoing frames and lets a test push
 * incoming ones back into the handlers the wiki store registers on import.
 * `vi.hoisted` because the `vi.mock` factory below is lifted above imports.
 */
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
      for (const handler of handlers.get(type) ?? []) {
        handler({ type, ...payload } as unknown as Message);
      }
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
    off(type: string, callback?: (msg: any) => void) {
      if (!callback) {
        bus.handlers.delete(type);
        return;
      }
      const list = (bus.handlers.get(type) ?? []).filter((entry) => entry !== callback);
      bus.handlers.set(type, list);
    }
  }
}));

/**
 * The store is a module-level singleton that wires up its frame handlers and
 * its request-id counter at import time, so every test gets a fresh instance.
 */
async function loadWiki() {
  bus.reset();
  vi.resetModules();
  const [{ wiki }, { connection }] = await Promise.all([import('./wiki'), import('./connection')]);
  return { wiki, connection };
}

/** Let pending promise chains settle without advancing the clock. */
function tick() {
  return vi.advanceTimersByTimeAsync(0);
}

/** The frames sent so far of one type, in order. */
function sentOf(type: string) {
  return bus.sent.filter((frame) => frame.type === type);
}

const PAGE = {
  slug: 'home',
  title: 'Home',
  revision: 3,
  updatedBy: 'alice',
  updatedAt: '2026-07-01T00:00:00Z',
  body: '# Home',
  author: 'bob',
  createdAt: '2026-06-01T00:00:00Z'
};

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('wiki — request tracking', () => {
  it('tags each request with a fresh id and resolves the matching reply', async () => {
    const { wiki } = await loadWiki();

    const first = wiki.getPage(1, 'home');
    const second = wiki.getPage(1, 'other');

    const frames = sentOf('wiki-get');
    expect(frames).toHaveLength(2);
    expect(frames[0]).toMatchObject({ channelId: 1, slug: 'home' });
    expect(frames[0].requestId).not.toBe(frames[1].requestId);

    // Answer out of order: replies are matched by id, not by arrival.
    bus.emit('wiki-page', { requestId: frames[1].requestId, page: null });
    bus.emit('wiki-page', { requestId: frames[0].requestId, page: PAGE });

    await expect(second).resolves.toBeNull();
    await expect(first).resolves.toEqual(PAGE);
  });

  it('ignores replies with an unknown, missing or malformed request id', async () => {
    const { wiki } = await loadWiki();

    const pending = wiki.getPage(1, 'home');
    const { requestId } = sentOf('wiki-get')[0];

    bus.emit('wiki-page', { requestId: requestId + 999, page: PAGE });
    bus.emit('wiki-page', { page: PAGE });
    bus.emit('wiki-page', { requestId: 'nope', page: PAGE });
    await tick();

    // Still pending — none of the above matched.
    bus.emit('wiki-page', { requestId, page: PAGE });
    await expect(pending).resolves.toEqual(PAGE);
  });

  it('rejects a request the server never answers and ignores a late reply', async () => {
    const { wiki } = await loadWiki();

    // The catch is attached up front: the rejection happens inside the timer
    // advance below, and an assertion added afterwards would be too late.
    const pending = wiki.getPage(1, 'home').catch((error: Error) => error);
    const { requestId } = sentOf('wiki-get')[0];

    await vi.advanceTimersByTimeAsync(REQUEST_TIMEOUT_MS);
    expect(await pending).toMatchObject({ message: 'Wiki request timed out' });

    // The entry is gone, so a late reply must not throw or double-resolve.
    expect(() => bus.emit('wiki-page', { requestId, page: PAGE })).not.toThrow();
  });

  it('does not reuse a request id across pending maps', async () => {
    const { wiki } = await loadWiki();

    // Left deliberately unanswered; they only exist to claim request ids.
    for (const pending of [
      wiki.getPage(1, 'home'),
      wiki.save(1, 'home', 'Home', 'body', 3),
      wiki.resolveLinks([{ channel: 'general', slug: 'home' }])
    ]) {
      pending.catch(() => {});
    }

    const ids = bus.sent.map((frame) => frame.requestId);
    expect(ids).toHaveLength(3);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe('wiki — saving', () => {
  it('resolves with the new revision on wiki-saved', async () => {
    const { wiki } = await loadWiki();

    const pending = wiki.save(1, 'home', 'Home', 'body', 3);
    const frame = sentOf('wiki-update')[0];
    expect(frame).toMatchObject({
      channelId: 1,
      slug: 'home',
      title: 'Home',
      body: 'body',
      expectedRevision: 3
    });

    bus.emit('wiki-saved', { requestId: frame.requestId, revision: 4 });
    await expect(pending).resolves.toEqual({ ok: true, revision: 4 });
  });

  it('resolves with the current page on wiki-conflict', async () => {
    const { wiki } = await loadWiki();

    const pending = wiki.save(1, 'home', 'Home', 'body', 3);
    const { requestId } = sentOf('wiki-update')[0];

    bus.emit('wiki-conflict', { requestId, page: PAGE });
    await expect(pending).resolves.toEqual({ ok: false, current: PAGE });
  });

  it('rejects a conflict whose page payload is unusable', async () => {
    const { wiki } = await loadWiki();

    const pending = wiki.save(1, 'home', 'Home', 'body', 3);
    const { requestId } = sentOf('wiki-update')[0];

    bus.emit('wiki-conflict', { requestId, page: { slug: 'home' } });
    await expect(pending).rejects.toThrow('Malformed wiki conflict response');
  });

  it('fills in defaults for missing optional page fields', async () => {
    const { wiki } = await loadWiki();

    const pending = wiki.getPage(1, 'home');
    const { requestId } = sentOf('wiki-get')[0];

    bus.emit('wiki-page', { requestId, page: { slug: 'home', title: 'Home' } });
    await expect(pending).resolves.toEqual({
      slug: 'home',
      title: 'Home',
      revision: 0,
      updatedBy: '',
      updatedAt: '',
      body: '',
      author: '',
      createdAt: ''
    });
  });
});

describe('wiki — link resolution', () => {
  function respondToLastResolve(exists = true) {
    const frames = sentOf('wiki-resolve');
    const frame = frames[frames.length - 1];
    bus.emit('wiki-resolved', {
      requestId: frame.requestId,
      results: frame.links.map((link: any) => ({ ...link, exists }))
    });
    return frame;
  }

  it('deduplicates links and chunks them to the server limit', async () => {
    const { wiki } = await loadWiki();

    const links = Array.from({ length: 120 }, (_, i) => ({
      channel: 'general',
      slug: `page-${i}`
    }));
    // Duplicates must not inflate the chunk count.
    const pending = wiki.resolveLinks([...links, ...links.slice(0, 30)]);

    respondToLastResolve();
    await tick();
    respondToLastResolve();
    await tick();
    respondToLastResolve();
    await tick();

    const frames = sentOf('wiki-resolve');
    expect(frames.map((f) => f.links.length)).toEqual([MAX_RESOLVE_LINKS, MAX_RESOLVE_LINKS, 20]);

    const result = await pending;
    expect(result.size).toBe(120);
    expect(result.get('general/page-0')).toBe(true);
    expect(result.get('general/page-119')).toBe(true);
  });

  it('serves repeat lookups from the cache without a second request', async () => {
    const { wiki } = await loadWiki();

    const first = wiki.resolveLinks([
      { channel: 'general', slug: 'a' },
      { channel: 'general', slug: 'b' }
    ]);
    const frame = sentOf('wiki-resolve')[0];
    bus.emit('wiki-resolved', {
      requestId: frame.requestId,
      results: [
        { channel: 'general', slug: 'a', exists: true },
        { channel: 'general', slug: 'b', exists: false }
      ]
    });
    await expect(first).resolves.toEqual(
      new Map([
        ['general/a', true],
        ['general/b', false]
      ])
    );

    const second = await wiki.resolveLinks([{ channel: 'general', slug: 'a' }]);
    expect(second).toEqual(new Map([['general/a', true]]));
    expect(sentOf('wiki-resolve')).toHaveLength(1);
  });

  it('drops malformed entries out of a wiki-resolved payload', async () => {
    const { wiki } = await loadWiki();

    const pending = wiki.resolveLinks([{ channel: 'general', slug: 'a' }]);
    const frame = sentOf('wiki-resolve')[0];
    bus.emit('wiki-resolved', {
      requestId: frame.requestId,
      results: [null, { channel: 'general' }, { slug: 'a' }, { channel: 'general', slug: 'a' }]
    });

    // `exists` is only true when it is literally `true`; the last entry omits it.
    await expect(pending).resolves.toEqual(new Map([['general/a', false]]));
  });

  it('invalidates the cache when an index arrives or a channel disappears', async () => {
    const { wiki } = await loadWiki();

    wiki.resolveLinks([{ channel: 'general', slug: 'a' }]);
    respondToLastResolve();
    await tick();
    expect(sentOf('wiki-resolve')).toHaveLength(1);

    // A page may have been created, renamed or deleted.
    bus.emit('wiki-index', { channelId: 1, pages: [] });
    wiki.resolveLinks([{ channel: 'general', slug: 'a' }]);
    expect(sentOf('wiki-resolve')).toHaveLength(2);
    respondToLastResolve();
    await tick();

    bus.emit('channel-remove', { channelId: 1 });
    wiki.resolveLinks([{ channel: 'general', slug: 'a' }]);
    expect(sentOf('wiki-resolve')).toHaveLength(3);
    respondToLastResolve();
    await tick();
  });
});

describe('wiki — index', () => {
  it('replaces a channel index wholesale and skips malformed pages', async () => {
    const { wiki } = await loadWiki();

    bus.emit('wiki-index', {
      channelId: 1,
      pages: [
        { slug: 'home', title: 'Home', revision: 2, updatedBy: 'alice', updatedAt: 'then' },
        { slug: 'no-title' },
        null,
        { title: 'no slug' }
      ]
    });
    expect(get(wiki)).toEqual({
      1: [{ slug: 'home', title: 'Home', revision: 2, updatedBy: 'alice', updatedAt: 'then' }]
    });

    bus.emit('wiki-index', { channelId: 2, pages: [{ slug: 'a', title: 'A' }] });
    expect(Object.keys(get(wiki))).toEqual(['1', '2']);

    bus.emit('wiki-index', { channelId: 1, pages: [] });
    expect(get(wiki)[1]).toEqual([]);
  });

  it('ignores index frames that are not shaped like an index', async () => {
    const { wiki } = await loadWiki();

    bus.emit('wiki-index', { channelId: '1', pages: [] });
    bus.emit('wiki-index', { channelId: 1 });
    expect(get(wiki)).toEqual({});
  });

  it('forgets a channel that was removed', async () => {
    const { wiki } = await loadWiki();

    bus.emit('wiki-index', { channelId: 1, pages: [{ slug: 'a', title: 'A' }] });
    bus.emit('channel-remove', { channelId: 2 });
    expect(Object.keys(get(wiki))).toEqual(['1']);

    bus.emit('channel-remove', { channelId: 1 });
    expect(get(wiki)).toEqual({});
  });
});

describe('wiki — connection lifecycle', () => {
  it('clears the index and rejects in-flight requests on reconnect', async () => {
    const { wiki, connection } = await loadWiki();

    bus.emit('wiki-index', { channelId: 1, pages: [{ slug: 'a', title: 'A' }] });
    const page = wiki.getPage(1, 'a');
    const save = wiki.save(1, 'a', 'A', 'body', 1);
    const links = wiki.resolveLinks([{ channel: 'general', slug: 'a' }]);

    connection.set('connecting');

    await expect(page).rejects.toThrow('Connection reset');
    await expect(save).rejects.toThrow('Connection reset');
    await expect(links).rejects.toThrow('Connection reset');
    expect(get(wiki)).toEqual({});
  });

  it('leaves state alone while the connection is up', async () => {
    const { wiki, connection } = await loadWiki();

    bus.emit('wiki-index', { channelId: 1, pages: [{ slug: 'a', title: 'A' }] });
    connection.set('connected');
    connection.set('disconnected');
    expect(get(wiki)[1]).toHaveLength(1);
  });

  it('drops the request timer along with the rejected request', async () => {
    const { wiki, connection } = await loadWiki();

    const page = wiki.getPage(1, 'a');
    connection.set('connecting');
    await expect(page).rejects.toThrow('Connection reset');

    // A leftover timer would fire an unhandled rejection here.
    await vi.advanceTimersByTimeAsync(REQUEST_TIMEOUT_MS * 2);
  });
});
