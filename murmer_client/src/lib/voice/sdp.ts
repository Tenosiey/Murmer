/**
 * Opus feature negotiation.
 *
 * Two encoder features are worth having and neither is on by default in a
 * plain offer, so they are written into the SDP by hand:
 *
 *  - `usedtx=1`      – discontinuous transmission. Silence stops costing a
 *                      packet every 20 ms and becomes a comfort-noise frame
 *                      every ~400 ms. Voice is a full mesh here and the
 *                      transmission gate feeds the encoder *digital zero*
 *                      while muted or not transmitting, so on a busy channel
 *                      most of the uplinks are idle most of the time.
 *  - `useinbandfec=1` – each packet carries a low-bitrate copy of the previous
 *                      frame, so a single loss is reconstructed instead of
 *                      concealed. Chromium already advertises this; writing it
 *                      out pins it against a future default change.
 *
 * These fmtp parameters are **receiver-to-sender preferences** (RFC 7587 §6):
 * the copy we send configures the *peer's* encoder, not ours. Both ends run
 * this client, so munging our own offer and answer covers both directions —
 * a peer that did not send `usedtx=1` simply gets a stream without DTX.
 *
 * SDP munging is not sanctioned by the WebRTC spec, which is why the rewrite
 * lives here as a pure function: it is unit-tested, it only ever appends to
 * `a=fmtp` lines of audio m-sections, and it returns the input untouched when
 * anything about the description is unexpected. The caller falls back to the
 * original SDP if it throws anyway — degraded audio beats a call that cannot
 * be negotiated at all.
 */

/** What we ask every peer's Opus encoder to do. */
export const OPUS_PARAMS = 'usedtx=1;useinbandfec=1';

/** `a=rtpmap:111 opus/48000/2` -> the payload type carrying Opus. */
const OPUS_RTPMAP = /^a=rtpmap:(\d+)\s+opus\/48000(?:\/2)?\s*$/i;

/**
 * Add `params` to the Opus fmtp line of every audio m-section in `sdp`.
 *
 * Idempotent: a parameter already present (whatever its value) is left alone,
 * so re-negotiating a connection cannot stack `usedtx=1;usedtx=1`.
 */
export function withOpusParams(sdp: string, params: string = OPUS_PARAMS): string {
  if (!sdp || !sdp.includes('m=audio')) return sdp;

  const wanted = params
    .split(';')
    .map((param) => param.trim())
    .filter(Boolean);
  if (wanted.length === 0) return sdp;

  // Keep the original line endings: an SDP uses CRLF, and a rewrite that
  // normalised them would change every line of the description.
  const lines = sdp.split(/(?<=\n)/);
  const out: string[] = [];
  /** Payload types of the audio section currently being walked. */
  let opusPayloads: string[] = [];
  /** Payload types whose fmtp line was already extended in this section. */
  let extended = new Set<string>();
  let inAudio = false;

  /** Insert an fmtp line for payload types that had none at all. */
  const flushMissingFmtp = () => {
    for (const payload of opusPayloads) {
      if (extended.has(payload)) continue;
      out.push(`a=fmtp:${payload} ${wanted.join(';')}\r\n`);
    }
    opusPayloads = [];
    extended = new Set();
  };

  for (const line of lines) {
    const text = line.trim();

    if (text.startsWith('m=')) {
      // A new media section ends the previous one; anything still pending
      // there never had an fmtp line to extend.
      flushMissingFmtp();
      inAudio = text.startsWith('m=audio');
      out.push(line);
      continue;
    }

    if (inAudio) {
      const rtpmap = OPUS_RTPMAP.exec(text);
      if (rtpmap) {
        opusPayloads.push(rtpmap[1]);
        out.push(line);
        continue;
      }

      const fmtp = /^a=fmtp:(\d+)\s+(.*)$/.exec(text);
      if (fmtp && opusPayloads.includes(fmtp[1])) {
        extended.add(fmtp[1]);
        out.push(line.replace(text, `a=fmtp:${fmtp[1]} ${mergeParams(fmtp[2], wanted)}`));
        continue;
      }
    }

    out.push(line);
  }

  flushMissingFmtp();
  return out.join('');
}

/** Existing fmtp payload plus the parameters it does not carry yet. */
function mergeParams(existing: string, wanted: string[]): string {
  const present = new Set(
    existing
      .split(';')
      .map((param) => param.trim().split('=')[0].toLowerCase())
      .filter(Boolean)
  );
  const additions = wanted.filter((param) => !present.has(param.split('=')[0].toLowerCase()));
  if (additions.length === 0) return existing.trim();
  return [existing.trim(), ...additions].join(';');
}

/**
 * `withOpusParams` for a session description on its way in or out. Never
 * throws: a description that cannot be rewritten is used as it came.
 */
export function withOpusFeatures<T extends { type?: unknown; sdp?: string | null }>(
  description: T
): T {
  try {
    if (!description?.sdp) return description;
    const sdp = withOpusParams(description.sdp);
    if (sdp === description.sdp) return description;
    return { ...description, sdp };
  } catch (error) {
    console.warn('Failed to negotiate Opus DTX/FEC, using the description as-is:', error);
    return description;
  }
}
