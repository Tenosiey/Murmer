/*
  End-to-end encryption for direct messages.

  Each user's long-term Ed25519 identity key (keypair.ts) doubles as their DM
  encryption key: both sides convert it to X25519 with ed2curve and encrypt
  with nacl.box (X25519 + XSalsa20-Poly1305). The box shared secret is
  symmetric — ECDH(mySecret, peerPublic) equals ECDH(peerSecret, myPublic) —
  so a single ciphertext is readable by exactly the two participants, and the
  sender can decrypt their own sent history with the same peer key. The
  server only relays and stores nonce/ciphertext pairs.

  Trust model: the server is the key directory (first key to claim a name is
  bound permanently). A malicious server could substitute keys on first
  contact; stores/peerKeys.ts pins keys on first use and flags changes, and
  dmFingerprint() supports out-of-band verification. There is no forward
  secrecy — a stolen keypair decrypts all past DMs.
*/
import nacl from 'tweetnacl';
import ed2curve from 'ed2curve';
import { fromBase64, toBase64 } from './keypair';

export interface EncryptedDm {
  /** Base64 of the 24-byte NaCl box nonce. */
  nonce: string;
  /** Base64 of the box ciphertext (plaintext + 16-byte authenticator). */
  ciphertext: string;
}

/** Convert both parties' Ed25519 keys to the X25519 pair nacl.box needs.
 *  Returns null if either key is malformed (e.g. a corrupted binding). */
function dhKeys(
  peerEdPublicKey: string,
  myEdSecretKey: string
): { peerPublic: Uint8Array; mySecret: Uint8Array } | null {
  try {
    const peerPublic = ed2curve.convertPublicKey(fromBase64(peerEdPublicKey));
    const mySecret = ed2curve.convertSecretKey(fromBase64(myEdSecretKey));
    return peerPublic && mySecret ? { peerPublic, mySecret } : null;
  } catch {
    return null;
  }
}

/** Encrypt a DM for the peer (and, via the shared secret, for ourselves). */
export function encryptDm(
  text: string,
  peerEdPublicKey: string,
  myEdSecretKey: string
): EncryptedDm | null {
  const keys = dhKeys(peerEdPublicKey, myEdSecretKey);
  if (!keys) return null;
  const nonce = nacl.randomBytes(nacl.box.nonceLength);
  const box = nacl.box(new TextEncoder().encode(text), nonce, keys.peerPublic, keys.mySecret);
  return { nonce: toBase64(nonce), ciphertext: toBase64(box) };
}

/** Decrypt a DM sent to or by us in a conversation with the given peer.
 *  Returns null when the payload does not authenticate (wrong key, tampered
 *  or corrupted data) — callers render a placeholder in that case. */
export function decryptDm(
  nonce: string,
  ciphertext: string,
  peerEdPublicKey: string,
  myEdSecretKey: string
): string | null {
  const keys = dhKeys(peerEdPublicKey, myEdSecretKey);
  if (!keys) return null;
  try {
    const opened = nacl.box.open(
      fromBase64(ciphertext),
      fromBase64(nonce),
      keys.peerPublic,
      keys.mySecret
    );
    return opened ? new TextDecoder().decode(opened) : null;
  } catch {
    return null;
  }
}

/** Short fingerprint over both parties' identity keys for out-of-band
 *  comparison. Order-independent, so it reads the same on both ends. */
export function dmFingerprint(keyA: string, keyB: string): string {
  const [first, second] = [keyA, keyB].sort();
  const digest = nacl.hash(new TextEncoder().encode(`${first}|${second}`));
  const hex = [...digest.slice(0, 8)]
    .map((b) => b.toString(16).padStart(2, '0').toUpperCase())
    .join('');
  return hex.replace(/(.{4})(?=.)/g, '$1 ');
}
