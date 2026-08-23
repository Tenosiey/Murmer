/**
 * Drafts are invisible until somebody switches away and back, so the failure
 * mode is a sentence that quietly disappears — or worse, one that reappears
 * addressed to the wrong conversation. Both are covered here: the park/take
 * handoff, and the per-server namespacing that keeps two servers' channel
 * ids (and two servers' account names) apart.
 */
import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

/** The store is a module-level singleton; each test takes a fresh one. */
async function loadDrafts() {
  vi.resetModules();
  return await import('./drafts');
}

describe('drafts — parking and taking', () => {
  it('hands a parked draft back and forgets it', async () => {
    const { drafts, channelDraft } = await loadDrafts();
    drafts.setServer('wss://one.example/ws');

    drafts.park(channelDraft(1), 'half a sen');
    expect(drafts.take(channelDraft(1))).toBe('half a sen');

    // The composer owns the text from here; the store must not keep a stale
    // second copy that a later switch would restore over the live one.
    expect(get(drafts)).toEqual({});
    expect(drafts.take(channelDraft(1))).toBe('');
  });

  it('returns an empty string for a conversation with nothing parked', async () => {
    const { drafts, channelDraft } = await loadDrafts();
    drafts.setServer('wss://one.example/ws');

    expect(drafts.take(channelDraft(99))).toBe('');
  });

  it('drops the entry when an emptied composer is parked', async () => {
    const { drafts, channelDraft } = await loadDrafts();
    drafts.setServer('wss://one.example/ws');

    drafts.park(channelDraft(1), 'typed');
    drafts.park(channelDraft(1), '');
    expect(get(drafts)).toEqual({});

    // Every channel visited parks on the way out, so an empty park must not
    // add a key or the map grows one entry per channel ever opened.
    drafts.park(channelDraft(2), '');
    expect(get(drafts)).toEqual({});
  });

  it('keeps channels, threads and DMs in separate keys', async () => {
    const { drafts, channelDraft, dmDraft, threadDraft } = await loadDrafts();
    drafts.setServer('wss://one.example/ws');

    drafts.park(channelDraft(1), 'in the channel');
    drafts.park(threadDraft(1), 'in the thread');
    drafts.park(dmDraft('alice'), 'to alice');
    drafts.park(dmDraft('bob'), 'to bob');

    expect(drafts.take(channelDraft(1))).toBe('in the channel');
    expect(drafts.take(threadDraft(1))).toBe('in the thread');
    expect(drafts.take(dmDraft('alice'))).toBe('to alice');
    expect(drafts.take(dmDraft('bob'))).toBe('to bob');
  });
});

describe('drafts — per-server namespacing', () => {
  it('keeps one server’s drafts out of another', async () => {
    const { drafts, channelDraft, dmDraft } = await loadDrafts();

    drafts.setServer('wss://one.example/ws');
    drafts.park(channelDraft(7), 'for one');
    drafts.park(dmDraft('alice'), 'to one’s alice');

    // Same channel id and the same account name, different server.
    drafts.setServer('wss://two.example/ws');
    expect(get(drafts)).toEqual({});
    expect(drafts.take(channelDraft(7))).toBe('');
    expect(drafts.take(dmDraft('alice'))).toBe('');

    drafts.setServer('wss://one.example/ws');
    expect(drafts.take(channelDraft(7))).toBe('for one');
    expect(drafts.take(dmDraft('alice'))).toBe('to one’s alice');
  });

  it('restores what was parked when reconnecting to the same server', async () => {
    const { drafts, channelDraft } = await loadDrafts();

    drafts.setServer('wss://one.example/ws');
    drafts.park(channelDraft(3), 'survives a dropped socket');

    // A retry after a connection drop runs connect() again with the same URL.
    drafts.setServer('wss://one.example/ws');
    expect(drafts.take(channelDraft(3))).toBe('survives a dropped socket');
  });

  it('never persists to localStorage', async () => {
    // An encrypted channel's key material is deliberately memory-only
    // (`channelKeys.ts`); writing its draft to disk would put the plaintext
    // somewhere the ciphertext never reaches.
    const { drafts, channelDraft } = await loadDrafts();
    const setItem = vi.spyOn(localStorage, 'setItem');

    drafts.setServer('wss://one.example/ws');
    drafts.park(channelDraft(1), 'secret');

    expect(setItem).not.toHaveBeenCalled();
    setItem.mockRestore();
  });
});
