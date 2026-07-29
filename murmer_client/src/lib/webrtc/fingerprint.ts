/**
 * The DTLS certificate fingerprint a description carries, lowercased, or
 * `null` when it has none.
 *
 * Used to tell two kinds of re-offer apart. An ICE restart renegotiates the
 * *same* connection and keeps the certificate, while a peer that gave up and
 * built a fresh `RTCPeerConnection` presents a new one — and that offer cannot
 * be answered on the old connection. The fingerprint is the only part of the
 * offer that says which of the two just arrived.
 *
 * Session-level and media-level fingerprints are both accepted (browsers put
 * it in either place); the first one found wins, since a description that
 * mixes certificates across m-lines is not something we produce or answer.
 */
export function remoteFingerprint(sdp: string | null | undefined): string | null {
  if (!sdp) return null;
  const match = /^a=fingerprint:(.+)$/m.exec(sdp);
  return match ? match[1].trim().toLowerCase() : null;
}
