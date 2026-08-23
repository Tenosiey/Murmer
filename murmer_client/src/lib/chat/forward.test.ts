/*
  Forwarding is the one place the client composes a message on somebody else's
  behalf, and the two halves of it fail invisibly in different ways.

  The destination list decides what a forward can even be pointed at, and both
  omissions here are things the server would otherwise refuse *after* the user
  picked them: an encrypted channel has no key the server can seal a copy
  with, and the channel the message is already in is not a destination at all.

  The DM text is the attribution itself — a DM has no structured field to put
  one in — so an author whose nickname contains markdown, or a forward of a
  forward that re-credits the wrong person, silently rewrites who said what.
*/
import { describe, expect, it } from 'vitest';
import type { ChannelInfo, Message } from '../types';
import { forwardOptions, forwardedDmText, parseForwardTarget } from './forward';

const identity = (user: string) => user;

const channel = (id: number, name: string, extra: Partial<ChannelInfo> = {}): ChannelInfo => ({
  id,
  name,
  categoryId: null,
  position: 0,
  ...extra
});

describe('parseForwardTarget', () => {
  it('reads back what forwardOptions wrote', () => {
    expect(parseForwardTarget('channel:12')).toEqual({ kind: 'channel', channelId: 12 });
    expect(parseForwardTarget('dm:alice')).toEqual({ kind: 'dm', user: 'alice' });
  });

  it('keeps a colon inside an account name', () => {
    // The name is the identity everywhere else, so it has to survive the
    // round trip intact rather than being cut at the first separator.
    expect(parseForwardTarget('dm:od:in')).toEqual({ kind: 'dm', user: 'od:in' });
  });

  it('rejects a cancelled dialog and anything malformed', () => {
    for (const raw of [null, '', 'channel', 'channel:', 'channel:abc', 'dm:', 'other:1']) {
      expect(parseForwardTarget(raw)).toBeNull();
    }
  });
});

describe('forwardOptions', () => {
  it('omits encrypted channels and the channel the message is already in', () => {
    const options = forwardOptions({
      channels: [
        channel(1, 'general'),
        channel(2, 'here'),
        channel(3, 'sealed', { e2ee: true, private: true })
      ],
      currentChannelId: 2,
      peers: [],
      displayName: identity
    });
    expect(options.map((option) => option.label)).toEqual(['#general']);
  });

  it('offers people after channels, under their display name', () => {
    const options = forwardOptions({
      channels: [channel(1, 'general')],
      currentChannelId: 9,
      peers: ['alice'],
      displayName: (user) => (user === 'alice' ? 'Alice A.' : user)
    });
    expect(options).toEqual([
      { value: 'channel:1', label: '#general', description: undefined },
      { value: 'dm:alice', label: 'Alice A.', description: 'Direct message' }
    ]);
  });
});

describe('forwardedDmText', () => {
  it('credits the author and the channel the words came from', () => {
    const msg: Message = { type: 'chat', user: 'alice', text: 'hello' };
    expect(forwardedDmText(msg, 'general', identity)).toBe(
      '↪ Forwarded from **alice** in **#general**\n\nhello'
    );
  });

  it('keeps the first author when forwarding a forward', () => {
    // Matches what the server stamps on a channel forward: the words are still
    // the first author's, whoever has passed them on since.
    const msg: Message = {
      type: 'chat',
      user: 'bob',
      text: 'hello',
      forwardedFrom: { id: 4, user: 'alice', channel: 'general', channelId: 1 }
    };
    expect(forwardedDmText(msg, 'random', identity)).toBe(
      '↪ Forwarded from **alice** in **#general**\n\nhello'
    );
  });

  it('escapes markdown in a name so it cannot become formatting', () => {
    const msg: Message = { type: 'dm', from: 'mallory', text: 'hi' };
    expect(forwardedDmText(msg, null, () => '**admin**')).toBe(
      '↪ Forwarded from **\\*\\*admin\\*\\***\n\nhi'
    );
  });

  it('carries an attachment across as a link rather than losing it', () => {
    // A DM holds text and nothing else, so the alternative is a forward that
    // silently drops the file it was sent for.
    const msg: Message = {
      type: 'chat',
      user: 'alice',
      attachment: { url: 'https://host/files/report.pdf', name: 'report.pdf', size: 10 }
    };
    expect(forwardedDmText(msg, 'general', identity)).toBe(
      '↪ Forwarded from **alice** in **#general**\n\n[report.pdf](https://host/files/report.pdf)'
    );
  });

  it('still attributes a message with no body at all', () => {
    const msg: Message = { type: 'chat', user: 'alice', image: 'https://host/files/cat.png' };
    expect(forwardedDmText(msg, null, identity)).toBe(
      '↪ Forwarded from **alice**\n\nhttps://host/files/cat.png'
    );
  });
});
