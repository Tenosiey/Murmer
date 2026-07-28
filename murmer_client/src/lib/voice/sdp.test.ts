import { describe, expect, it } from 'vitest';
import { OPUS_PARAMS, withOpusFeatures, withOpusParams } from './sdp';

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

/** The parameters carried by the fmtp line of `payload`. */
function fmtpOf(sdp: string, payload: string): string | null {
  const match = new RegExp(`^a=fmtp:${payload} (.*)$`, 'm').exec(sdp);
  return match ? match[1].trim() : null;
}

describe('withOpusParams', () => {
  it('extends the Opus fmtp line without dropping what it carried', () => {
    const sdp = audioSdp(['a=rtpmap:111 opus/48000/2', 'a=fmtp:111 minptime=10;useinbandfec=1']);

    const result = withOpusParams(sdp);

    // The parameter Chromium already advertises is kept as it was — only the
    // missing one is added, and never a second copy of an existing one.
    expect(fmtpOf(result, '111')).toBe('minptime=10;useinbandfec=1;usedtx=1');
  });

  it('writes an fmtp line when the description has none', () => {
    const sdp = audioSdp(['a=rtpmap:111 opus/48000/2', 'a=rtcp-fb:111 transport-cc']);

    const result = withOpusParams(sdp);

    expect(fmtpOf(result, '111')).toBe(OPUS_PARAMS);
    // The inserted line belongs to the audio section it describes.
    expect(result.indexOf('a=fmtp:111')).toBeGreaterThan(result.indexOf('m=audio'));
  });

  it('is idempotent, so a re-negotiated peer cannot stack parameters', () => {
    const sdp = audioSdp(['a=rtpmap:111 opus/48000/2', 'a=fmtp:111 minptime=10']);

    const once = withOpusParams(sdp);
    const twice = withOpusParams(once);

    expect(twice).toBe(once);
    expect(fmtpOf(twice, '111')?.match(/usedtx/g)).toHaveLength(1);
  });

  it('leaves a value the peer already chose alone', () => {
    const sdp = audioSdp(['a=rtpmap:111 opus/48000/2', 'a=fmtp:111 usedtx=0;useinbandfec=1']);

    // Overwriting would be munging away an explicit decision; the parameter is
    // present, so it is not ours to change.
    expect(fmtpOf(withOpusParams(sdp), '111')).toBe('usedtx=0;useinbandfec=1');
  });

  it('touches neither other codecs nor other media', () => {
    const sdp = [
      audioSdp([
        'a=rtpmap:111 opus/48000/2',
        'a=fmtp:111 minptime=10',
        'a=rtpmap:126 telephone-event/8000',
        'a=fmtp:126 0-16'
      ]),
      'm=video 9 UDP/TLS/RTP/SAVPF 96',
      'a=mid:1',
      'a=rtpmap:96 VP8/90000',
      'a=fmtp:96 max-fs=12288',
      ''
    ].join('\r\n');

    const result = withOpusParams(sdp);

    expect(fmtpOf(result, '126')).toBe('0-16');
    expect(fmtpOf(result, '96')).toBe('max-fs=12288');
  });

  it('covers every audio section of a multi-track description', () => {
    const sdp = [
      audioSdp(['a=rtpmap:111 opus/48000/2', 'a=fmtp:111 minptime=10']),
      'm=audio 9 UDP/TLS/RTP/SAVPF 109',
      'a=mid:1',
      'a=rtpmap:109 opus/48000/2',
      'a=fmtp:109 minptime=10',
      ''
    ].join('\r\n');

    const result = withOpusParams(sdp);

    expect(fmtpOf(result, '111')).toContain('usedtx=1');
    expect(fmtpOf(result, '109')).toContain('usedtx=1');
  });

  it('keeps the CRLF line endings an SDP is made of', () => {
    const sdp = audioSdp(['a=rtpmap:111 opus/48000/2', 'a=rtcp-fb:111 transport-cc']);

    const result = withOpusParams(sdp);

    expect(result.split('\n').every((line) => line === '' || line.endsWith('\r'))).toBe(true);
  });

  it('returns descriptions it finds nothing to do in unchanged', () => {
    const noAudio = 'v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=rtpmap:96 VP8/90000\r\n';
    const noOpus = audioSdp(['a=rtpmap:8 PCMA/8000']);

    expect(withOpusParams(noAudio)).toBe(noAudio);
    expect(withOpusParams(noOpus)).toBe(noOpus);
    expect(withOpusParams('')).toBe('');
  });
});

describe('withOpusFeatures', () => {
  it('rewrites the description in place, keeping its type', () => {
    const description = {
      type: 'offer' as const,
      sdp: audioSdp(['a=rtpmap:111 opus/48000/2', 'a=fmtp:111 minptime=10'])
    };

    const result = withOpusFeatures(description);

    expect(result.type).toBe('offer');
    expect(fmtpOf(result.sdp, '111')).toContain('usedtx=1');
  });

  it('hands back anything it cannot rewrite instead of failing the call', () => {
    // A negotiation that cannot be munged still has to happen: audio without
    // DTX beats no connection at all.
    expect(withOpusFeatures({ type: 'answer', sdp: null })).toEqual({
      type: 'answer',
      sdp: null
    });
    expect(withOpusFeatures({ type: 'answer' })).toEqual({ type: 'answer' });
  });
});
