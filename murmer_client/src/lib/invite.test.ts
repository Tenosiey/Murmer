/**
 * An invite link carries a server password, so both halves matter: the
 * encoding has to survive characters a password may contain, and the parser
 * has to refuse anything that is not one of our own links — it is handed
 * whatever the OS passes to the `murmer://` handler, or whatever the user
 * pasted.
 */
import { describe, expect, it, vi } from 'vitest';
import { createInviteLink, parseInviteLink } from './invite';

describe('createInviteLink', () => {
  it('encodes url, name and password', () => {
    const link = createInviteLink({
      url: 'wss://example.com/ws',
      name: 'Example',
      password: 'hunter2'
    });
    const params = new URL(link).searchParams;
    expect(link.startsWith('murmer://invite?')).toBe(true);
    expect(params.get('url')).toBe('wss://example.com/ws');
    expect(params.get('name')).toBe('Example');
    expect(params.get('password')).toBe('hunter2');
  });

  it('omits a name that is just the URL again', () => {
    const link = createInviteLink({ url: 'wss://example.com/ws', name: 'wss://example.com/ws' });
    expect(new URL(link).searchParams.has('name')).toBe(false);
  });

  it('omits an absent or empty password', () => {
    expect(new URL(createInviteLink({ url: 'ws://a/ws', name: 'A' })).searchParams.has('password')).toBe(
      false
    );
    expect(
      new URL(createInviteLink({ url: 'ws://a/ws', name: 'A', password: '' })).searchParams.has(
        'password'
      )
    ).toBe(false);
  });

  it('round-trips values that would otherwise break the query string', () => {
    const server = {
      url: 'wss://example.com/ws',
      name: 'Ünicode & friends #1',
      password: 'p@ss word&name=evil#frag'
    };
    const parsed = parseInviteLink(createInviteLink(server));
    expect(parsed).toEqual(server);
  });
});

describe('parseInviteLink', () => {
  it('parses a full invite', () => {
    expect(parseInviteLink('murmer://invite?url=wss%3A%2F%2Fexample.com%2Fws&name=Example')).toEqual(
      { url: 'wss://example.com/ws', name: 'Example' }
    );
  });

  it('normalizes the server URL it was given', () => {
    // Invites are hand-edited and shared as text; a bare host has to end up on
    // the same URL the server list would have stored.
    expect(parseInviteLink('murmer://invite?url=example.com')?.url).toBe('ws://example.com/ws');
    expect(parseInviteLink('murmer://invite?url=https%3A%2F%2Fexample.com')?.url).toBe(
      'wss://example.com/ws'
    );
  });

  it('accepts the schemeless-authority spelling some launchers hand over', () => {
    expect(parseInviteLink('murmer:invite?url=example.com')?.url).toBe('ws://example.com/ws');
    expect(parseInviteLink('murmer://invite/?url=example.com')?.url).toBe('ws://example.com/ws');
  });

  it('trims surrounding whitespace and a blank name', () => {
    expect(parseInviteLink('  murmer://invite?url=example.com&name=%20%20  ')).toEqual({
      url: 'ws://example.com/ws'
    });
    expect(parseInviteLink('murmer://invite?url=example.com&name=%20Example%20')?.name).toBe(
      'Example'
    );
  });

  it('rejects links that are not murmer invites', () => {
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    try {
      const rejected = [
        '',
        '   ',
        'not a link',
        'https://example.com/invite?url=example.com',
        'murmer://join?url=example.com',
        'murmer://invite',
        'murmer://invite?name=Example',
        'murmer://invite?url=',
        'javascript:alert(1)'
      ];
      for (const link of rejected) {
        expect(parseInviteLink(link), link).toBeNull();
      }
    } finally {
      errors.mockRestore();
    }
  });

  it('keeps a password out of the parsed entry when the link carries none', () => {
    const parsed = parseInviteLink('murmer://invite?url=example.com&password=');
    expect(parsed).toEqual({ url: 'ws://example.com/ws' });
    expect(parsed && 'password' in parsed).toBe(false);
  });
});
