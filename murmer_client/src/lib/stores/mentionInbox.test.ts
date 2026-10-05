/**
 * The inbox merges two sources that overlap: the server's stored copy and
 * the live one. The ways that goes wrong are quiet — a mention listed twice,
 * an unseen count that grows every time the panel reloads, or a sealed
 * message whose only readable copy is overwritten by the server's blank one.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import {
  MAX_INBOX_ENTRIES,
  mentionInbox,
  mergeInbox,
  withServerAnswer,
  type InboxEntry
} from './mentionInbox';

function entry(id: number, text: string | null = `message ${id}`, liveOnly = false): InboxEntry {
  return { id, channelId: 1, user: 'bob', text, timestamp: null, liveOnly };
}

describe('mergeInbox', () => {
  it('keeps one entry per message, newest first', () => {
    const merged = mergeInbox([entry(3), entry(1)], [entry(2), entry(3, 'edited')]);
    expect(merged.map((e) => e.id)).toEqual([3, 2, 1]);
    expect(merged[0].text).toBe('edited');
  });

  it('never trades the readable copy of a sealed message for a blank one', () => {
    // The server cannot read an encrypted channel; the live copy could.
    const merged = mergeInbox([entry(5, 'opened locally')], [entry(5, null)]);
    expect(merged[0].text).toBe('opened locally');
  });

  it('keeps only the newest entries', () => {
    const many = Array.from({ length: MAX_INBOX_ENTRIES + 5 }, (_, i) => entry(i + 1));
    const merged = mergeInbox([], many);
    expect(merged).toHaveLength(MAX_INBOX_ENTRIES);
    expect(merged[0].id).toBe(MAX_INBOX_ENTRIES + 5);
  });
});

describe('withServerAnswer', () => {
  it('drops what the server could list but no longer does, keeps what it never could', () => {
    // 1 was deleted while the reader looked elsewhere; 2 is a name mention
    // in an encrypted channel, which the server cannot read.
    const held = [entry(1), entry(2, 'sealed but opened', true), entry(3)];
    const merged = withServerAnswer(held, [entry(3), entry(4)]);
    expect(merged.map((e) => e.id)).toEqual([4, 3, 2]);
  });
});

describe('mentionInbox', () => {
  beforeEach(() => mentionInbox.reset());

  it('counts a mention as unseen once, and only from elsewhere', () => {
    mentionInbox.add(entry(1), true);
    mentionInbox.add(entry(1), true);
    mentionInbox.add(entry(2), false);
    expect(get(mentionInbox).unseen).toBe(1);
    // Reloading from the server is not news.
    mentionInbox.load([entry(1), entry(2), entry(3)]);
    expect(get(mentionInbox).unseen).toBe(1);
    mentionInbox.markSeen();
    expect(get(mentionInbox).unseen).toBe(0);
  });

  it('drops a deleted message', () => {
    mentionInbox.load([entry(1), entry(2)]);
    mentionInbox.remove(1);
    expect(get(mentionInbox).entries.map((e) => e.id)).toEqual([2]);
  });
});
