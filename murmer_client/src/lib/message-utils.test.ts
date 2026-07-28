import { describe, expect, it } from 'vitest';
import type { Message } from './types';
import {
  containsMention,
  escapeRegex,
  normalizeAttachment,
  normalizeReactions,
  normalizeReplyTo,
  prepareMessage
} from './message-utils';

describe('normalizeAttachment', () => {
  it('accepts http and https attachments', () => {
    expect(normalizeAttachment({ url: 'https://host/files/a.png', name: 'a.png', size: 12 })).toEqual(
      { url: 'https://host/files/a.png', name: 'a.png', size: 12 }
    );
    expect(normalizeAttachment({ url: 'http://host/files/a.png', name: 'a.png' })).toEqual({
      url: 'http://host/files/a.png',
      name: 'a.png',
      size: 0
    });
  });

  it('rejects every scheme that could execute on click', () => {
    // A compromised or malicious server controls this field, and the UI turns
    // it into an href — anything but http(s) must never reach the DOM.
    const schemes = [
      'javascript:alert(1)',
      'JavaScript:alert(1)',
      'data:text/html;base64,PHNjcmlwdD4=',
      'vbscript:msgbox(1)',
      'file:///etc/passwd',
      'blob:https://host/1234',
      'about:blank'
    ];
    for (const url of schemes) {
      expect(normalizeAttachment({ url, name: 'a.png' })).toBeUndefined();
    }
  });

  it('rejects payloads that are not a usable attachment', () => {
    expect(normalizeAttachment(null)).toBeUndefined();
    expect(normalizeAttachment('https://host/a.png')).toBeUndefined();
    expect(normalizeAttachment({ name: 'a.png' })).toBeUndefined();
    expect(normalizeAttachment({ url: 'https://host/a.png' })).toBeUndefined();
    // Not parseable as an absolute URL.
    expect(normalizeAttachment({ url: 'files/a.png', name: 'a.png' })).toBeUndefined();
    // A name of only whitespace leaves nothing to render.
    expect(normalizeAttachment({ url: 'https://host/a.png', name: '   ' })).toBeUndefined();
  });

  it('trims the name and falls back to a zero size', () => {
    for (const size of [undefined, -1, Number.NaN, Number.POSITIVE_INFINITY, '12']) {
      expect(normalizeAttachment({ url: 'https://host/a.png', name: ' a.png ', size })).toEqual({
        url: 'https://host/a.png',
        name: 'a.png',
        size: 0
      });
    }
  });
});

describe('normalizeReplyTo', () => {
  it('keeps a well-formed reply and defaults its text fields', () => {
    expect(normalizeReplyTo({ id: 7, user: 'alice', text: 'hi' })).toEqual({
      id: 7,
      user: 'alice',
      text: 'hi'
    });
    expect(normalizeReplyTo({ id: 7 })).toEqual({ id: 7, user: '', text: '' });
    expect(normalizeReplyTo({ id: 7, user: 42, text: null })).toEqual({
      id: 7,
      user: '',
      text: ''
    });
  });

  it('requires a finite numeric id', () => {
    expect(normalizeReplyTo(null)).toBeUndefined();
    expect(normalizeReplyTo({})).toBeUndefined();
    expect(normalizeReplyTo({ id: '7' })).toBeUndefined();
    expect(normalizeReplyTo({ id: Number.NaN })).toBeUndefined();
  });
});

describe('normalizeReactions', () => {
  it('deduplicates users and drops unusable entries', () => {
    expect(
      normalizeReactions({
        '👍': ['alice', 'bob', 'alice'],
        '👎': ['alice', 42, null, '', '   '],
        '': ['alice'],
        '🎉': 'not an array'
      })
    ).toEqual({ '👍': ['alice', 'bob'], '👎': ['alice'] });
  });

  it('returns an empty map for anything that is not an object', () => {
    for (const value of [null, undefined, 'x', 7]) {
      expect(normalizeReactions(value)).toEqual({});
    }
  });
});

describe('prepareMessage', () => {
  const base: Message = { type: 'message', user: 'alice', text: 'hi' };

  it('normalizes a timestamp to ISO and derives a display time', () => {
    const msg = prepareMessage({ ...base, timestamp: '2026-07-01T10:00:00.000Z' });
    expect(msg.timestamp).toBe('2026-07-01T10:00:00.000Z');
    expect(msg.time).toBeTruthy();
  });

  it('drops an unparseable or non-string timestamp', () => {
    expect(prepareMessage({ ...base, timestamp: 'not a date' }).timestamp).toBeUndefined();
    expect(prepareMessage({ ...base, timestamp: 12345 as never }).timestamp).toBeUndefined();
  });

  it('keeps an explicit time instead of inventing one', () => {
    const msg = prepareMessage({ ...base, timestamp: '2026-07-01T10:00:00Z', time: '10:00:00' });
    expect(msg.time).toBe('10:00:00');
  });

  it('strips attachment and reply metadata that failed validation', () => {
    const msg = prepareMessage({
      ...base,
      attachment: { url: 'javascript:alert(1)', name: 'x' } as never,
      replyTo: { id: 'nope' } as never,
      threadId: 'nope' as never
    });
    expect('attachment' in msg).toBe(false);
    expect('replyTo' in msg).toBe(false);
    expect('threadId' in msg).toBe(false);
  });

  it('treats a valid expiry as ephemeral and drops an invalid one', () => {
    const expiring = prepareMessage({ ...base, expiresAt: '2026-07-01T10:00:00Z' });
    expect(expiring.expiresAt).toBe('2026-07-01T10:00:00.000Z');
    expect(expiring.ephemeral).toBe(true);

    const bogus = prepareMessage({ ...base, expiresAt: 'soon', ephemeral: true });
    expect('expiresAt' in bogus).toBe(false);
    // The explicit flag still stands on its own.
    expect(bogus.ephemeral).toBe(true);

    const plain = prepareMessage({ ...base, expiresAt: 'soon' });
    expect('ephemeral' in plain).toBe(false);
  });

  it('does not mutate the message it was given', () => {
    const raw: Message = { ...base, reactions: { '👍': ['alice', 'alice'] } };
    const copy = structuredClone(raw);
    prepareMessage(raw);
    expect(raw).toEqual(copy);
  });
});

describe('containsMention', () => {
  it('matches a mention anywhere in the text, case-insensitively', () => {
    expect(containsMention('@alice hello', 'alice')).toBe(true);
    expect(containsMention('hey @alice!', 'alice')).toBe(true);
    expect(containsMention('hey @Alice', 'alice')).toBe(true);
    expect(containsMention('hey @alice', 'Alice')).toBe(true);
  });

  it('does not fire on a longer name that merely starts the same', () => {
    expect(containsMention('hey @alicia', 'alice')).toBe(false);
    expect(containsMention('hey @alice-bob', 'alice')).toBe(false);
    expect(containsMention('hey @alice_bob', 'alice')).toBe(false);
  });

  it('does not fire on an address that merely contains the name', () => {
    expect(containsMention('mail bob@alice.example', 'alice')).toBe(false);
    expect(containsMention('@@alice', 'alice')).toBe(false);
  });

  it('treats a name with regex metacharacters literally', () => {
    // Account names come off the wire; an unescaped one would either throw or
    // silently match the wrong people.
    expect(containsMention('hi @a.b', 'a.b')).toBe(true);
    expect(containsMention('hi @axb', 'a.b')).toBe(false);
    expect(() => containsMention('hi', 'a(b')).not.toThrow();
    expect(containsMention('hi @a(b', 'a(b')).toBe(true);
  });

  it('is false without both a text and a name', () => {
    expect(containsMention(undefined, 'alice')).toBe(false);
    expect(containsMention('@alice', null)).toBe(false);
    expect(containsMention('@alice', '')).toBe(false);
    expect(containsMention('', 'alice')).toBe(false);
  });
});

describe('escapeRegex', () => {
  it('escapes every character that would change a pattern', () => {
    expect(escapeRegex('a.b*c')).toBe('a\\.b\\*c');
    expect(new RegExp(`^${escapeRegex('a[b]c')}$`).test('a[b]c')).toBe(true);
    expect(new RegExp(`^${escapeRegex('a[b]c')}$`).test('abc')).toBe(false);
  });

  it('leaves ordinary text untouched', () => {
    expect(escapeRegex('alice_99')).toBe('alice_99');
  });
});
