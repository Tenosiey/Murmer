import { describe, expect, it, vi } from 'vitest';

vi.mock('./chat', () => ({ chat: { on() {}, off() {} } }));
vi.mock('./connection', () => ({ connection: { subscribe: () => () => {} } }));

const { parseIceServers } = await import('./iceConfig');

describe('parseIceServers', () => {
  it('keeps STUN entries and drops everything else', () => {
    expect(
      parseIceServers([
        { urls: 'stun:a.example:3478' },
        { urls: 'stuns:b.example:5349', extra: 1 },
        // A TURN entry without credentials makes RTCPeerConnection throw.
        { urls: 'turn:relay.example:3478' },
        { urls: ['stun:array.example'] },
        null,
        'stun:bare'
      ])
    ).toEqual([{ urls: 'stun:a.example:3478' }, { urls: 'stuns:b.example:5349' }]);
  });

  it('treats a malformed frame as no servers', () => {
    expect(parseIceServers(undefined)).toEqual([]);
    expect(parseIceServers({ urls: 'stun:a' })).toEqual([]);
  });
});
