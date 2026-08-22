/*
  End-to-end encryption for private channels.

  A channel is not a pair of people, so it cannot reuse the DM construction:
  there is no two-party shared secret to derive. Instead an encrypted channel
  has one 32-byte symmetric key, and messages are sealed with nacl.secretbox
  (XSalsa20-Poly1305) under it. Distribution is the part that uses the identity
  keys: the channel key is *wrapped* for each member with nacl.box, exactly
  like a DM, so the server stores one opaque blob per member and can open none
  of them.

  Keys are versioned by an integer epoch, carried on every message. Admitting a
  member wraps the current key for them; removing one starts a new epoch that
  is simply never wrapped for them — which is what makes a removal take effect
  rather than being a UI change. Old epochs stay wrapped for the members who
  had them, so history keeps opening.

  What this does and does not buy, stated plainly:
  - The server stores and relays ciphertext only. An operator reading the
    database, or the traffic, learns who posted when and how long the message
    was, not what it said.
  - The server is still the *directory*: it decides who is on the member roster
    and hands out the identity keys the roster is wrapped for. A malicious
    server can therefore put a key it controls on the roster. stores/peerKeys.ts
    pins every member's key on first use and blocks wrapping for a changed key,
    which limits that to first contact, the same bound DMs have.
  - There is no forward secrecy within an epoch: whoever holds an epoch's key
    can read every message sent under it, forever.
  - Uploaded files are not covered. Their bytes live on the server; only the
    attachment's name and URL travel inside the ciphertext.
*/
import nacl from 'tweetnacl';
import { fromBase64, toBase64 } from './keypair';
import { dhKeys } from './dm-crypto';
import type { AttachmentInfo } from './types';

/** Bytes of a channel key. `nacl.secretbox` takes exactly this many. */
export const CHANNEL_KEY_LENGTH = nacl.secretbox.keyLength;

/** A channel key sealed for one member. */
export interface WrappedChannelKey {
  /** Base64 of the 24-byte NaCl box nonce. */
  nonce: string;
  /** Base64 of the box ciphertext over the raw channel key. */
  wrappedKey: string;
}

/** A message's sealed envelope as it travels on the wire and is stored. */
export interface SealedMessage {
  /** Which channel key version the ciphertext is under. */
  epoch: number;
  /** Base64 of the 24-byte secretbox nonce. */
  nonce: string;
  /** Base64 of the secretbox ciphertext. */
  ciphertext: string;
}

/**
 * Everything about a message that would otherwise sit in the clear on the
 * server. Kept a JSON object rather than a bare string so an attachment, an
 * image and a reply's quoted snippet are covered by the same seal as the text
 * — the server rebuilds reply quotes from stored plaintext, which an encrypted
 * channel has none of, so the quote travels here instead.
 */
export interface ChannelMessagePayload {
  text?: string;
  image?: string;
  attachment?: AttachmentInfo;
  /** Quoted snippet of the message being replied to. */
  replyText?: string;
}

/** A fresh random channel key, base64 encoded. */
export function generateChannelKey(): string {
  return toBase64(nacl.randomBytes(CHANNEL_KEY_LENGTH));
}

/** Seal a channel key for one member. Returns null if either key is malformed. */
export function wrapChannelKey(
  channelKey: string,
  recipientEdPublicKey: string,
  myEdSecretKey: string
): WrappedChannelKey | null {
  const keys = dhKeys(recipientEdPublicKey, myEdSecretKey);
  if (!keys) return null;
  let raw: Uint8Array;
  try {
    raw = fromBase64(channelKey);
  } catch {
    return null;
  }
  if (raw.length !== CHANNEL_KEY_LENGTH) return null;
  const nonce = nacl.randomBytes(nacl.box.nonceLength);
  const box = nacl.box(raw, nonce, keys.peerPublic, keys.mySecret);
  return { nonce: toBase64(nonce), wrappedKey: toBase64(box) };
}

/**
 * Open a wrap addressed to us. `senderEdPublicKey` is the member who wrapped
 * it — the box shared secret is symmetric, so their public key plus our secret
 * reproduces what they sealed with.
 *
 * Returns the base64 channel key, or null when the wrap does not authenticate
 * (a substituted sender key, a corrupted blob) or does not contain a key of
 * the right length.
 */
export function unwrapChannelKey(
  nonce: string,
  wrappedKey: string,
  senderEdPublicKey: string,
  myEdSecretKey: string
): string | null {
  const keys = dhKeys(senderEdPublicKey, myEdSecretKey);
  if (!keys) return null;
  try {
    const opened = nacl.box.open(
      fromBase64(wrappedKey),
      fromBase64(nonce),
      keys.peerPublic,
      keys.mySecret
    );
    if (!opened || opened.length !== CHANNEL_KEY_LENGTH) return null;
    return toBase64(opened);
  } catch {
    return null;
  }
}

/** Seal a message payload under a channel key. Returns null on a bad key. */
export function encryptChannelMessage(
  payload: ChannelMessagePayload,
  epoch: number,
  channelKey: string
): SealedMessage | null {
  let raw: Uint8Array;
  try {
    raw = fromBase64(channelKey);
  } catch {
    return null;
  }
  if (raw.length !== CHANNEL_KEY_LENGTH) return null;
  const nonce = nacl.randomBytes(nacl.secretbox.nonceLength);
  const box = nacl.secretbox(new TextEncoder().encode(JSON.stringify(payload)), nonce, raw);
  return { epoch, nonce: toBase64(nonce), ciphertext: toBase64(box) };
}

/**
 * Open a sealed message. Returns null when the ciphertext does not
 * authenticate under the key (wrong epoch's key, tampered data) or when the
 * plaintext is not the JSON object this module writes — callers render a
 * placeholder in either case rather than dropping the message, so a member who
 * cannot read one still sees that it exists.
 */
export function decryptChannelMessage(
  sealed: SealedMessage,
  channelKey: string
): ChannelMessagePayload | null {
  let raw: Uint8Array;
  try {
    raw = fromBase64(channelKey);
  } catch {
    return null;
  }
  if (raw.length !== CHANNEL_KEY_LENGTH) return null;
  let opened: Uint8Array | null;
  try {
    opened = nacl.secretbox.open(fromBase64(sealed.ciphertext), fromBase64(sealed.nonce), raw);
  } catch {
    return null;
  }
  if (!opened) return null;
  try {
    const parsed: unknown = JSON.parse(new TextDecoder().decode(opened));
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
    return parsed as ChannelMessagePayload;
  } catch {
    return null;
  }
}

/**
 * Read a sealed envelope off a server frame, rejecting anything that is not
 * one. Server frames are untrusted input like any other; a malformed `enc`
 * must read as "cannot decrypt", never as a partially-filled envelope.
 */
export function parseSealedMessage(value: unknown): SealedMessage | null {
  if (!value || typeof value !== 'object') return null;
  const raw = value as Record<string, unknown>;
  if (typeof raw.epoch !== 'number' || !Number.isInteger(raw.epoch) || raw.epoch < 1) return null;
  if (typeof raw.nonce !== 'string' || typeof raw.ciphertext !== 'string') return null;
  return { epoch: raw.epoch, nonce: raw.nonce, ciphertext: raw.ciphertext };
}

/**
 * Short fingerprint of a channel key for out-of-band comparison: two members
 * reading the same groups aloud are holding the same key, so nobody has been
 * handed a separate one.
 */
export function channelKeyFingerprint(channelKey: string): string {
  let raw: Uint8Array;
  try {
    raw = fromBase64(channelKey);
  } catch {
    return '';
  }
  const digest = nacl.hash(raw);
  const hex = [...digest.slice(0, 8)]
    .map((b) => b.toString(16).padStart(2, '0').toUpperCase())
    .join('');
  return hex.replace(/(.{4})(?=.)/g, '$1 ');
}
