/**
 * An invite link carries a credential — a server-issued invite code, or on an
 * older link the server password itself — so both halves matter: the encoding
 * has to survive characters a password may contain and keep the credential out
 * of the query string (a fragment never reaches a web server), and the parser
 * has to refuse anything that is not one of our own links — it is handed
 * whatever URL the browser was opened with, or whatever the user pasted into
 * the add-server field.
 *
 * The one rule that is a security property rather than a convenience: a link
 * built for an invite code must not also carry the password. A code expires,
 * runs out and can be revoked; a password shipped alongside it would survive
 * all three and quietly undo the revocation.
 */
import { describe, expect, it, vi } from 'vitest';
import { createInviteLink, looksLikeInviteLink, parseInviteLink } from './invite';

describe('createInviteLink', () => {
  it('points at the server itself when no web client origin is given', () => {
    const link = createInviteLink({ url: 'wss://example.com/ws', name: 'Example' });
    const url = new URL(link);
    expect(url.origin).toBe('https://example.com');
    expect(url.pathname).toBe('/invite');
  });

  it('points at the hosted web client when we are one', () => {
    const link = createInviteLink({ url: 'wss://example.com/ws', name: 'Example' }, 'https://chat.example.org');
    expect(link.startsWith('https://chat.example.org/invite#')).toBe(true);
  });

  it('encodes url, name and password in the fragment, never the query', () => {
    const link = createInviteLink({
      url: 'wss://example.com/ws',
      name: 'Example',
      password: 'hunter2'
    });
    const url = new URL(link);
    expect(url.search).toBe('');
    const params = new URLSearchParams(url.hash.slice(1));
    expect(params.get('url')).toBe('wss://example.com/ws');
    expect(params.get('name')).toBe('Example');
    expect(params.get('password')).toBe('hunter2');
  });

  it('omits a name that is just the URL again', () => {
    const link = createInviteLink({ url: 'wss://example.com/ws', name: 'wss://example.com/ws' });
    expect(new URLSearchParams(new URL(link).hash.slice(1)).has('name')).toBe(false);
  });

  it('omits an absent or empty password', () => {
    const params = (link: string) => new URLSearchParams(new URL(link).hash.slice(1));
    expect(params(createInviteLink({ url: 'ws://a/ws', name: 'A' })).has('password')).toBe(false);
    expect(params(createInviteLink({ url: 'ws://a/ws', name: 'A', password: '' })).has('password')).toBe(
      false
    );
  });

  it('round-trips values that would otherwise break the query string', () => {
    const server = {
      url: 'wss://example.com/ws',
      name: 'Ünicode & friends #1',
      password: 'p@ss word&name=evil#frag'
    };
    expect(parseInviteLink(createInviteLink(server))).toEqual(server);
  });

  it('carries a server-issued code instead of the password, never both', () => {
    const link = createInviteLink(
      { url: 'wss://example.com/ws', name: 'Example', password: 'hunter2' },
      undefined,
      'aGVsbG8td29ybGQ'
    );
    const params = new URLSearchParams(new URL(link).hash.slice(1));
    expect(params.get('invite')).toBe('aGVsbG8td29ybGQ');
    // Shipping the password too would let a revoked invite keep working.
    expect(params.has('password')).toBe(false);
  });

  it('falls back to the password when no code is given', () => {
    const link = createInviteLink(
      { url: 'wss://example.com/ws', name: 'Example', password: 'hunter2' },
      undefined,
      ''
    );
    const params = new URLSearchParams(new URL(link).hash.slice(1));
    expect(params.get('password')).toBe('hunter2');
    expect(params.has('invite')).toBe(false);
  });

  it('round-trips a code through the parser', () => {
    const server = { url: 'wss://example.com/ws', name: 'Example' };
    const link = createInviteLink(server, undefined, 'x-Y_09');
    expect(parseInviteLink(link)).toEqual({ ...server, code: 'x-Y_09' });
  });

  it('does not double the slash when the origin carries a trailing one', () => {
    expect(createInviteLink({ url: 'ws://a/ws', name: 'A' }, 'https://chat.example.org/')).toContain(
      'https://chat.example.org/invite#'
    );
  });
});

describe('parseInviteLink', () => {
  it('parses a full invite', () => {
    expect(parseInviteLink('https://chat.example.org/invite#url=wss%3A%2F%2Fexample.com%2Fws&name=Example')).toEqual(
      { url: 'wss://example.com/ws', name: 'Example' }
    );
  });

  it('normalizes the server URL it was given', () => {
    // Invites are hand-edited and shared as text; a bare host has to end up on
    // the same URL the server list would have stored.
    expect(parseInviteLink('https://a/invite#url=example.com')?.url).toBe('ws://example.com/ws');
    expect(parseInviteLink('https://a/invite#url=https%3A%2F%2Fexample.com')?.url).toBe(
      'wss://example.com/ws'
    );
  });

  it('accepts a hand-written query string and a trailing slash', () => {
    expect(parseInviteLink('http://a:3001/invite?url=example.com')?.url).toBe('ws://example.com/ws');
    expect(parseInviteLink('http://a:3001/invite/#url=example.com')?.url).toBe('ws://example.com/ws');
  });

  it('prefers the fragment over the query when both carry a url', () => {
    // Only the fragment is ours; a query pair could have been appended by a
    // link shortener or a chat client rewriting the URL.
    expect(parseInviteLink('https://a/invite?url=evil.example#url=good.example')?.url).toBe(
      'ws://good.example/ws'
    );
  });

  it('trims surrounding whitespace and a blank name', () => {
    expect(parseInviteLink('  https://a/invite#url=example.com&name=%20%20  ')).toEqual({
      url: 'ws://example.com/ws'
    });
    expect(parseInviteLink('https://a/invite#url=example.com&name=%20Example%20')?.name).toBe(
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
        'https://example.com/ws',
        'https://example.com/invited#url=example.com',
        'https://example.com/some/invite#url=example.com',
        'https://example.com/invite',
        'https://example.com/invite#name=Example',
        'https://example.com/invite#url=',
        'https://example.com/invite#url=%20',
        'murmer://invite?url=example.com',
        'file:///invite#url=example.com',
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
    const parsed = parseInviteLink('https://a/invite#url=example.com&password=');
    expect(parsed).toEqual({ url: 'ws://example.com/ws' });
    expect(parsed && 'password' in parsed).toBe(false);
  });

  it('keeps a code out of the parsed entry when the link carries none', () => {
    for (const link of [
      'https://a/invite#url=example.com',
      'https://a/invite#url=example.com&invite=',
      'https://a/invite#url=example.com&invite=%20%20'
    ]) {
      const parsed = parseInviteLink(link);
      expect(parsed, link).toEqual({ url: 'ws://example.com/ws' });
      expect(parsed && 'code' in parsed, link).toBe(false);
    }
  });

  it('accepts a link that carries both, which an older server still needs', () => {
    // `createInviteLink` never builds one, but a hand-written or hand-edited
    // link may; dropping either half would strand whoever was sent it.
    expect(parseInviteLink('https://a/invite#url=example.com&password=pw&invite=code')).toEqual({
      url: 'ws://example.com/ws',
      password: 'pw',
      code: 'code'
    });
  });
});

describe('looksLikeInviteLink', () => {
  it('separates a broken invite from a server address', () => {
    // The add-server field normalizes anything that is not an invite into a
    // hostname, so a malformed invite has to be caught before it is stored as
    // `wss://host/invite/ws`.
    expect(looksLikeInviteLink('https://example.com/invite')).toBe(true);
    expect(looksLikeInviteLink('https://example.com/invite#name=Example')).toBe(true);
    expect(looksLikeInviteLink('https://example.com')).toBe(false);
    expect(looksLikeInviteLink('example.com:3001')).toBe(false);
    expect(looksLikeInviteLink('wss://example.com/ws')).toBe(false);
  });
});
