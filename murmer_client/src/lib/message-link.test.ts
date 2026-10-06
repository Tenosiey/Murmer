/*
 * Message links arrive from other people, so parsing them is untrusted input:
 * anything that is not exactly a link to one message on one server must come
 * back null rather than half-parsed into a jump somewhere unexpected.
 */
import { describe, expect, it } from 'vitest';
import { createMessageLink, parseMessageLink } from './message-link';

const link = { server: 'wss://chat.example.com/ws', channel: 3, message: 42 };

describe('message links', () => {
  it('round-trips through the server origin and an app origin', () => {
    const own = createMessageLink(link);
    expect(own.startsWith('https://chat.example.com/chat#')).toBe(true);
    expect(parseMessageLink(own)).toEqual(link);

    const hosted = createMessageLink(link, 'https://app.example.org/');
    expect(hosted.startsWith('https://app.example.org/chat#')).toBe(true);
    expect(parseMessageLink(hosted)).toEqual(link);
  });

  it('normalizes the server the way the server list does', () => {
    const parsed = parseMessageLink(
      'https://x.test/chat#server=https%3A%2F%2Fchat.example.com&channel=3&message=42'
    );
    expect(parsed?.server).toBe('wss://chat.example.com/ws');
  });

  it('rejects anything that is not a whole message link', () => {
    for (const href of [
      'not a url',
      'https://chat.example.com/',
      'https://chat.example.com/invite#url=wss%3A%2F%2Fa%2Fws',
      'javascript:alert(1)//chat#server=a&channel=1&message=1',
      'https://chat.example.com/chat#channel=1&message=1',
      'https://chat.example.com/chat#server=a&channel=0&message=1',
      'https://chat.example.com/chat#server=a&channel=1&message=-4',
      'https://chat.example.com/chat#server=a&channel=1.5&message=1',
      'https://chat.example.com/chat#server=a&channel=1'
    ]) {
      expect(parseMessageLink(href), href).toBeNull();
    }
  });
});
