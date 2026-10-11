/**
 * `normalizeServerUrl` decides what every connection, every per-server
 * storage namespace and every uploaded file URL is keyed on, so two spellings
 * of the same server that normalize differently silently split a user's
 * history in two.
 */
import { describe, expect, it } from 'vitest';
import { isLocalHost, isUnencryptedRemote, normalizeServerUrl } from './utils';

describe('normalizeServerUrl', () => {
  it('turns a bare public host into a wss:// endpoint', () => {
    expect(normalizeServerUrl('example.com')).toBe('wss://example.com/ws');
    expect(normalizeServerUrl('example.com:3000')).toBe('wss://example.com:3000/ws');
    expect(normalizeServerUrl('203.0.113.7:3001')).toBe('wss://203.0.113.7:3001/ws');
  });

  it('keeps ws:// for a bare local or LAN host', () => {
    expect(normalizeServerUrl('localhost:3001')).toBe('ws://localhost:3001/ws');
    expect(normalizeServerUrl('127.0.0.1:8080')).toBe('ws://127.0.0.1:8080/ws');
    expect(normalizeServerUrl('192.168.1.20:3001')).toBe('ws://192.168.1.20:3001/ws');
    expect(normalizeServerUrl('homeserver:3001')).toBe('ws://homeserver:3001/ws');
    expect(normalizeServerUrl('[::1]:3001')).toBe('ws://[::1]:3001/ws');
  });

  it('trims surrounding whitespace and a trailing slash', () => {
    expect(normalizeServerUrl('  example.com  ')).toBe('wss://example.com/ws');
    expect(normalizeServerUrl('example.com/')).toBe('wss://example.com/ws');
    expect(normalizeServerUrl('\texample.com:3000/\n')).toBe('wss://example.com:3000/ws');
  });

  it('maps http to ws and https to wss', () => {
    expect(normalizeServerUrl('http://example.com')).toBe('ws://example.com/ws');
    expect(normalizeServerUrl('https://example.com')).toBe('wss://example.com/ws');
    expect(normalizeServerUrl('https://example.com/')).toBe('wss://example.com/ws');
    expect(normalizeServerUrl('https://example.com:8443')).toBe('wss://example.com:8443/ws');
  });

  it('keeps a path prefix and appends /ws below it', () => {
    // Servers behind a reverse proxy are mounted under a sub-path.
    expect(normalizeServerUrl('https://example.com/murmer')).toBe('wss://example.com/murmer/ws');
    expect(normalizeServerUrl('https://example.com/murmer/')).toBe('wss://example.com/murmer/ws');
  });

  it('does not append a second /ws to a URL that already ends in one', () => {
    expect(normalizeServerUrl('https://example.com/ws')).toBe('wss://example.com/ws');
    expect(normalizeServerUrl('http://example.com/murmer/ws')).toBe('ws://example.com/murmer/ws');
  });

  it('passes an explicit ws:// or wss:// URL through untouched', () => {
    // A user who typed the scheme themselves is trusted with the whole URL —
    // including one that does not end in /ws, which is why the ws:// form is
    // not a shortcut for "normalize this for me".
    expect(normalizeServerUrl('wss://example.com/ws')).toBe('wss://example.com/ws');
    expect(normalizeServerUrl('ws://example.com/socket')).toBe('ws://example.com/socket');
    expect(normalizeServerUrl('  ws://example.com  ')).toBe('ws://example.com');
  });

  it('is idempotent, so a stored URL never renormalizes into a different one', () => {
    const inputs = [
      'example.com',
      'example.com/',
      'example.com:3000',
      'http://example.com',
      'https://example.com/',
      'https://example.com/murmer',
      'wss://example.com/ws'
    ];
    for (const input of inputs) {
      const once = normalizeServerUrl(input);
      expect(normalizeServerUrl(once)).toBe(once);
    }
  });
});

describe('isLocalHost', () => {
  it('accepts loopback, private ranges, link-local and LAN names', () => {
    for (const host of [
      'localhost', '127.0.0.1', '10.0.0.5', '172.16.0.1', '172.31.255.255',
      '192.168.0.1', '169.254.1.1', '::1', 'fd12:3456::1', 'fe80::1',
      'nas.local', 'homeserver'
    ]) {
      expect(isLocalHost(host), host).toBe(true);
    }
  });

  it('rejects public addresses and names', () => {
    for (const host of ['example.com', '8.8.8.8', '172.32.0.1', '2001:db8::1', '']) {
      expect(isLocalHost(host), host).toBe(false);
    }
  });
});

describe('isUnencryptedRemote', () => {
  it('flags only ws:// to a public host', () => {
    expect(isUnencryptedRemote('ws://example.com/ws')).toBe(true);
    expect(isUnencryptedRemote('wss://example.com/ws')).toBe(false);
    expect(isUnencryptedRemote('ws://localhost:3001/ws')).toBe(false);
    expect(isUnencryptedRemote('ws://192.168.1.20:3001/ws')).toBe(false);
  });
});
