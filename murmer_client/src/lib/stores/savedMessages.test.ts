import { describe, expect, it } from 'vitest';
import { parseSaved, savedEntry, toggleSaved, type SavedMessage } from './savedMessages';

/**
 * The list is written to disk, so the one rule that matters most is that an
 * encrypted channel's text never goes into an entry.
 */
const entry = (id: number): SavedMessage => ({
  id,
  channelId: 1,
  user: 'alice',
  text: `message ${id}`,
  timestamp: null
});

describe('savedEntry', () => {
  it('keeps the text of a plain channel', () => {
    expect(savedEntry({ type: 'chat', id: 4, user: 'bob', text: 'hi', time: 't' }, 2, false)).toEqual({
      id: 4,
      channelId: 2,
      user: 'bob',
      text: 'hi',
      timestamp: 't'
    });
  });

  it('never keeps the text of an encrypted channel', () => {
    expect(savedEntry({ type: 'chat', id: 4, user: 'bob', text: 'secret' }, 2, true)?.text).toBeNull();
  });

  it('needs a stored message', () => {
    expect(savedEntry({ type: 'chat', user: 'bob', text: 'pending' }, 2, false)).toBeNull();
  });
});

describe('toggleSaved', () => {
  it('saves to the front and unsaves on the second toggle', () => {
    const saved = toggleSaved([entry(1)], entry(2));
    expect(saved.map((e) => e.id)).toEqual([2, 1]);
    expect(toggleSaved(saved, entry(1)).map((e) => e.id)).toEqual([2]);
  });

  it('drops the oldest save past the cap', () => {
    expect(toggleSaved([entry(2), entry(1)], entry(3), 2).map((e) => e.id)).toEqual([3, 2]);
  });
});

describe('parseSaved', () => {
  it('drops malformed entries and survives garbage', () => {
    const raw = JSON.stringify({ 'ws://a': [entry(1), { id: 'x' }], 'ws://b': 'nope' });
    expect(parseSaved(raw)).toEqual({ 'ws://a': [entry(1)] });
    expect(parseSaved('{not json')).toEqual({});
    expect(parseSaved(null)).toEqual({});
  });
});
