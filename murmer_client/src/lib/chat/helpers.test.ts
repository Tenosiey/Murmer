/**
 * Message grouping and the ephemeral countdown are the two bits of the chat
 * page that decide what the user sees without anything visibly failing when
 * they are wrong: a broken group renders a redundant header instead of an
 * error, and a broken expiry label counts down to the wrong moment for a
 * message that really is about to disappear.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Message } from '../types';
import {
  buildMessageBlocks,
  describeDuration,
  ephemeralInfo,
  formatExpiry,
  formatFileSize,
  formatShortTime,
  pingToStrength,
  parseTimestampValue,
  reactionEntries,
  searchResultPreview
} from './helpers';

/** Local wall-clock timestamp, so day grouping does not depend on the TZ. */
function at(
  year: number,
  month: number,
  day: number,
  hour = 12,
  minute = 0,
  second = 0
): string {
  return new Date(year, month - 1, day, hour, minute, second).toISOString();
}

function message(overrides: Partial<Message> = {}): Message {
  return { type: 'chat', user: 'alice', text: 'hello', ...overrides };
}

function kinds(blocks: ReturnType<typeof buildMessageBlocks>): string[] {
  return blocks.map((block) => block.kind);
}

afterEach(() => {
  vi.useRealTimers();
});

describe('buildMessageBlocks', () => {
  it('groups consecutive messages from the same author inside the window', () => {
    const blocks = buildMessageBlocks([
      message({ id: 1, timestamp: at(2026, 3, 4, 10, 0) }),
      message({ id: 2, timestamp: at(2026, 3, 4, 10, 2) }),
      message({ id: 3, timestamp: at(2026, 3, 4, 10, 4, 59) })
    ]);
    const messages = blocks.filter((block) => block.kind === 'message');
    expect(messages.map((block) => block.continuation)).toEqual([false, true, true]);
  });

  it('starts a new group once the five-minute window is exceeded', () => {
    const blocks = buildMessageBlocks([
      message({ id: 1, timestamp: at(2026, 3, 4, 10, 0) }),
      message({ id: 2, timestamp: at(2026, 3, 4, 10, 5, 1) })
    ]);
    const messages = blocks.filter((block) => block.kind === 'message');
    expect(messages.map((block) => block.continuation)).toEqual([false, false]);
  });

  it('measures the window from the head of the group, not the previous message', () => {
    // Otherwise a steady stream of one message every four minutes would stay
    // one group forever.
    const blocks = buildMessageBlocks([
      message({ id: 1, timestamp: at(2026, 3, 4, 10, 0) }),
      message({ id: 2, timestamp: at(2026, 3, 4, 10, 4) }),
      message({ id: 3, timestamp: at(2026, 3, 4, 10, 8) })
    ]);
    const messages = blocks.filter((block) => block.kind === 'message');
    expect(messages.map((block) => block.continuation)).toEqual([false, true, false]);
  });

  it('breaks the group when the author changes', () => {
    const blocks = buildMessageBlocks([
      message({ id: 1, user: 'alice', timestamp: at(2026, 3, 4, 10, 0) }),
      message({ id: 2, user: 'bob', timestamp: at(2026, 3, 4, 10, 1) }),
      message({ id: 3, user: 'alice', timestamp: at(2026, 3, 4, 10, 2) })
    ]);
    const messages = blocks.filter((block) => block.kind === 'message');
    expect(messages.map((block) => block.continuation)).toEqual([false, false, false]);
  });

  it('never continues a group with a reply', () => {
    // A reply renders its quote and needs its own header above it.
    const blocks = buildMessageBlocks([
      message({ id: 1, timestamp: at(2026, 3, 4, 10, 0) }),
      message({
        id: 2,
        timestamp: at(2026, 3, 4, 10, 1),
        replyTo: { id: 1, user: 'alice', text: 'hello' }
      }),
      message({ id: 3, timestamp: at(2026, 3, 4, 10, 2) })
    ]);
    const messages = blocks.filter((block) => block.kind === 'message');
    expect(messages.map((block) => block.continuation)).toEqual([false, false, true]);
  });

  it('does not group messages that carry no parsable timestamp', () => {
    const blocks = buildMessageBlocks([
      message({ id: 1, timestamp: 'not a date' }),
      message({ id: 2, timestamp: 'not a date' })
    ]);
    const messages = blocks.filter((block) => block.kind === 'message');
    expect(messages.map((block) => block.continuation)).toEqual([false, false]);
    expect(kinds(blocks)).toEqual(['message', 'message']);
  });

  it('inserts one day separator per calendar day and breaks the group there', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 2, 5, 15, 0));
    const blocks = buildMessageBlocks([
      message({ id: 1, timestamp: at(2026, 3, 3, 23, 59) }),
      message({ id: 2, timestamp: at(2026, 3, 4, 0, 1) }),
      message({ id: 3, timestamp: at(2026, 3, 4, 0, 2) }),
      message({ id: 4, timestamp: at(2026, 3, 5, 9, 0) })
    ]);
    expect(kinds(blocks)).toEqual([
      'separator',
      'message',
      'separator',
      'message',
      'message',
      'separator',
      'message'
    ]);
    const separators = blocks.filter((block) => block.kind === 'separator');
    expect(separators.map((block) => block.label)).toEqual([
      expect.stringContaining('2026'),
      'Yesterday',
      'Today'
    ]);
    // The first message of a new day always renders its header.
    const messages = blocks.filter((block) => block.kind === 'message');
    expect(messages.map((block) => block.continuation)).toEqual([false, false, true, false]);
  });

  it('places the unread marker before the first foreign message above the id', () => {
    const blocks = buildMessageBlocks(
      [
        message({ id: 5, user: 'alice', timestamp: at(2026, 3, 4, 10, 0) }),
        message({ id: 6, user: 'alice', timestamp: at(2026, 3, 4, 10, 1) }),
        message({ id: 7, user: 'bob', timestamp: at(2026, 3, 4, 10, 2) })
      ],
      { unreadAfterId: 5, currentUser: 'bob' }
    );
    const index = blocks.findIndex((block) => block.kind === 'unread');
    expect(index).toBeGreaterThan(-1);
    const next = blocks[index + 1];
    expect(next.kind === 'message' && next.message.id).toBe(6);
    expect(blocks.filter((block) => block.kind === 'unread')).toHaveLength(1);
  });

  it('never marks the viewer’s own messages as unread', () => {
    const blocks = buildMessageBlocks(
      [
        message({ id: 6, user: 'bob', timestamp: at(2026, 3, 4, 10, 0) }),
        message({ id: 7, user: 'alice', timestamp: at(2026, 3, 4, 10, 1) })
      ],
      { unreadAfterId: 5, currentUser: 'bob' }
    );
    const index = blocks.findIndex((block) => block.kind === 'unread');
    const next = blocks[index + 1];
    expect(next.kind === 'message' && next.message.id).toBe(7);
  });

  it('breaks the group at the unread marker', () => {
    // A continuation directly under the marker would hide who is speaking.
    const blocks = buildMessageBlocks(
      [
        message({ id: 5, timestamp: at(2026, 3, 4, 10, 0) }),
        message({ id: 6, timestamp: at(2026, 3, 4, 10, 1) })
      ],
      { unreadAfterId: 5, currentUser: 'bob' }
    );
    const messages = blocks.filter((block) => block.kind === 'message');
    expect(messages.map((block) => block.continuation)).toEqual([false, false]);
  });

  it('omits the marker when nothing is unread', () => {
    const messages = [message({ id: 5, timestamp: at(2026, 3, 4, 10, 0) })];
    expect(kinds(buildMessageBlocks(messages))).not.toContain('unread');
    expect(kinds(buildMessageBlocks(messages, { unreadAfterId: 0 }))).not.toContain('unread');
    expect(kinds(buildMessageBlocks(messages, { unreadAfterId: 9 }))).not.toContain('unread');
  });

  it('gives every block a distinct key, even for messages with no id', () => {
    const blocks = buildMessageBlocks(
      [
        message({ id: 5, timestamp: at(2026, 3, 4, 10, 0) }),
        message({ id: 6, timestamp: at(2026, 3, 4, 10, 1) }),
        message({ timestamp: at(2026, 3, 5, 10, 0), time: '10:00:00' }),
        message({ timestamp: at(2026, 3, 5, 10, 1), time: '10:01:00' })
      ],
      { unreadAfterId: 5, currentUser: 'bob' }
    );
    const keys = blocks.map((block) => block.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it('returns nothing for an empty history', () => {
    expect(buildMessageBlocks([])).toEqual([]);
  });

  it('extracts each message’s links once and reuses them', () => {
    const first = message({ id: 1, text: 'see https://example.com/a and https://example.com/a' });
    const blocks = buildMessageBlocks([first]);
    const again = buildMessageBlocks([first]);
    const links = blocks[0].kind === 'message' ? blocks[0].links : [];
    expect(links).toEqual(['https://example.com/a']);
    // Cached per message object, so the second pass hands back the same array.
    expect(again[0].kind === 'message' && again[0].links).toBe(links);
  });
});

describe('formatExpiry', () => {
  const now = Date.parse('2026-03-04T10:00:00Z');
  const inSeconds = (seconds: number) => new Date(now + seconds * 1000).toISOString();

  it('returns null when there is nothing to count down', () => {
    expect(formatExpiry(undefined, now)).toBeNull();
    expect(formatExpiry('', now)).toBeNull();
    expect(formatExpiry('whenever', now)).toBeNull();
  });

  it('says Expired at and after the deadline', () => {
    expect(formatExpiry(inSeconds(0), now)).toBe('Expired');
    expect(formatExpiry(inSeconds(-1), now)).toBe('Expired');
  });

  it('counts seconds, then minutes, hours and days', () => {
    expect(formatExpiry(inSeconds(1), now)).toBe('Expires in 1s');
    expect(formatExpiry(inSeconds(59), now)).toBe('Expires in 59s');
    expect(formatExpiry(inSeconds(60), now)).toBe('Expires in 1m');
    expect(formatExpiry(inSeconds(90), now)).toBe('Expires in 1m 30s');
    expect(formatExpiry(inSeconds(3600), now)).toBe('Expires in 1h');
    expect(formatExpiry(inSeconds(3600 + 120), now)).toBe('Expires in 1h 2m');
    expect(formatExpiry(inSeconds(24 * 3600), now)).toBe('Expires in 1d');
    expect(formatExpiry(inSeconds(25 * 3600), now)).toBe('Expires in 1d 1h');
    expect(formatExpiry(inSeconds(72 * 3600), now)).toBe('Expires in 3d');
  });

  it('rounds sub-second remainders instead of showing 0s', () => {
    expect(formatExpiry(new Date(now + 400).toISOString(), now)).toBe('Expires in 0s');
    expect(formatExpiry(new Date(now + 1600).toISOString(), now)).toBe('Expires in 2s');
  });
});

describe('ephemeralInfo', () => {
  const now = Date.parse('2026-03-04T10:00:00Z');

  it('is null for a message that does not expire', () => {
    expect(ephemeralInfo({ type: 'chat' }, now)).toBeNull();
    expect(ephemeralInfo({ type: 'chat', expiresAt: 42 as unknown as string }, now)).toBeNull();
  });

  it('pairs the countdown with an absolute timestamp for the tooltip', () => {
    const expiresAt = new Date(now + 90_000).toISOString();
    const info = ephemeralInfo({ type: 'chat', expiresAt }, now);
    expect(info?.label).toBe('Expires in 1m 30s');
    expect(info?.absolute).toBe(new Date(expiresAt).toLocaleString());
  });
});

describe('describeDuration', () => {
  it('spells out seconds, minutes and hours with singular/plural units', () => {
    expect(describeDuration(1)).toBe('1 second');
    expect(describeDuration(45)).toBe('45 seconds');
    expect(describeDuration(60)).toBe('1 minute');
    expect(describeDuration(90)).toBe('1 minute 30 seconds');
    expect(describeDuration(121)).toBe('2 minutes 1 second');
    expect(describeDuration(3600)).toBe('1 hour');
    expect(describeDuration(3660)).toBe('1 hour 1 minute');
    expect(describeDuration(7320)).toBe('2 hours 2 minutes');
    expect(describeDuration(0)).toBe('0 seconds');
  });
});

describe('parseTimestampValue', () => {
  it('parses an ISO timestamp and rejects anything unparsable', () => {
    expect(parseTimestampValue('2026-03-04T10:00:00Z')?.toISOString()).toBe(
      '2026-03-04T10:00:00.000Z'
    );
    expect(parseTimestampValue(undefined)).toBeNull();
    expect(parseTimestampValue('')).toBeNull();
    expect(parseTimestampValue('sometime')).toBeNull();
  });
});

describe('searchResultPreview', () => {
  it('prefers the text, collapsed onto one line', () => {
    expect(searchResultPreview(message({ text: '  hello\n\n  world  ' }))).toBe('hello world');
  });

  it('truncates long text with an ellipsis', () => {
    const preview = searchResultPreview(message({ text: 'x'.repeat(400) }));
    expect(preview).toHaveLength(118);
    expect(preview.endsWith('…')).toBe(true);
  });

  it('falls back to the attachment kind when there is no text', () => {
    expect(searchResultPreview(message({ text: '   ', image: 'https://host/a.png' }))).toBe(
      '[Image]'
    );
    expect(
      searchResultPreview(
        message({ text: undefined, attachment: { url: 'https://host/a.pdf', name: 'a.pdf', size: 1 } })
      )
    ).toBe('[File] a.pdf');
    expect(searchResultPreview(message({ text: undefined }))).toBe('Message');
  });
});

describe('formatFileSize', () => {
  it('scales bytes to KB and MB', () => {
    expect(formatFileSize(512)).toBe('512 B');
    expect(formatFileSize(1024)).toBe('1.0 KB');
    expect(formatFileSize(20 * 1024)).toBe('20 KB');
    expect(formatFileSize(1024 * 1024)).toBe('1.0 MB');
    expect(formatFileSize(20 * 1024 * 1024)).toBe('20 MB');
  });

  it('renders nothing for a size the server did not send', () => {
    for (const bytes of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(formatFileSize(bytes)).toBe('');
    }
  });
});

describe('formatShortTime', () => {
  it('drops the seconds off a pre-formatted time when there is no timestamp', () => {
    expect(formatShortTime(message({ time: '9:07:42' }))).toBe('9:07');
    expect(formatShortTime(message({ time: '21:07:42 PM' }))).toBe('21:07 PM');
    expect(formatShortTime(message({}))).toBe('');
  });
});

describe('reactionEntries', () => {
  it('lists reactions and drops the ones nobody holds', () => {
    expect(
      reactionEntries(message({ reactions: { '👍': ['alice', 'bob'], '🔥': [], '🎉': ['carol'] } }))
    ).toEqual([
      { emoji: '👍', users: ['alice', 'bob'] },
      { emoji: '🎉', users: ['carol'] }
    ]);
    expect(reactionEntries(message())).toEqual([]);
    expect(reactionEntries(undefined)).toEqual([]);
  });
});

describe('pingToStrength', () => {
  it('maps round-trip time onto five bars', () => {
    expect(pingToStrength(0)).toBe(5);
    expect(pingToStrength(49)).toBe(5);
    expect(pingToStrength(50)).toBe(4);
    expect(pingToStrength(99)).toBe(4);
    expect(pingToStrength(100)).toBe(3);
    expect(pingToStrength(200)).toBe(2);
    expect(pingToStrength(400)).toBe(1);
    expect(pingToStrength(5000)).toBe(1);
  });
});
