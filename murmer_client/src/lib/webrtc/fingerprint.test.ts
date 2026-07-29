import { describe, expect, it } from 'vitest';
import { remoteFingerprint } from './fingerprint';

/** A description shaped like the ones Chromium produces, CRLF and all. */
function audioSdp(lines: string[]): string {
  return [
    'v=0',
    'o=- 4611731400430051336 2 IN IP4 127.0.0.1',
    's=-',
    't=0 0',
    'm=audio 9 UDP/TLS/RTP/SAVPF 111 63 126',
    'c=IN IP4 0.0.0.0',
    'a=mid:0',
    ...lines,
    ''
  ].join('\r\n');
}

describe('remoteFingerprint', () => {
  it('reads a session-level fingerprint', () => {
    const sdp = audioSdp(['a=fingerprint:sha-256 AB:CD:EF', 'a=rtpmap:111 opus/48000/2']);

    expect(remoteFingerprint(sdp)).toBe('sha-256 ab:cd:ef');
  });

  it('tells two certificates apart and matches a description against itself', () => {
    const restart = audioSdp(['a=fingerprint:sha-256 AB:CD:EF', 'a=ice-ufrag:newufrag']);
    const rebuilt = audioSdp(['a=fingerprint:sha-256 11:22:33', 'a=ice-ufrag:newufrag']);
    const original = audioSdp(['a=fingerprint:sha-256 ab:cd:ef']);

    // An ICE restart keeps the certificate even though everything else about
    // the transport changed; only a fresh peer connection brings a new one.
    expect(remoteFingerprint(restart)).toBe(remoteFingerprint(original));
    expect(remoteFingerprint(rebuilt)).not.toBe(remoteFingerprint(original));
  });

  it('returns null for a description that carries none, and for no description', () => {
    expect(remoteFingerprint(audioSdp(['a=rtpmap:111 opus/48000/2']))).toBeNull();
    expect(remoteFingerprint(null)).toBeNull();
    expect(remoteFingerprint(undefined)).toBeNull();
    expect(remoteFingerprint('')).toBeNull();
  });
});
