import { describe, expect, it } from 'vitest';
import nacl from 'tweetnacl';
import {
  CHANNEL_KEY_LENGTH,
  channelKeyFingerprint,
  decryptChannelMessage,
  encryptChannelMessage,
  generateChannelKey,
  parseSealedMessage,
  unwrapChannelKey,
  wrapChannelKey
} from './channel-crypto';
import { fromBase64, toBase64 } from './keypair';

/** A fresh Ed25519 identity, the same shape `keypair.ts` persists. */
function identity() {
  const pair = nacl.sign.keyPair();
  return { publicKey: toBase64(pair.publicKey), secretKey: toBase64(pair.secretKey) };
}

const alice = identity();
const bob = identity();
const mallory = identity();

describe('generateChannelKey', () => {
  it('produces a key of exactly the length secretbox takes', () => {
    expect(fromBase64(generateChannelKey())).toHaveLength(CHANNEL_KEY_LENGTH);
  });

  it('never repeats a key', () => {
    const keys = new Set(Array.from({ length: 32 }, generateChannelKey));
    expect(keys.size).toBe(32);
  });
});

describe('wrapChannelKey / unwrapChannelKey', () => {
  it('hands the channel key to the member it was wrapped for', () => {
    const key = generateChannelKey();
    const wrapped = wrapChannelKey(key, bob.publicKey, alice.secretKey);
    expect(wrapped).not.toBeNull();
    expect(unwrapChannelKey(wrapped!.nonce, wrapped!.wrappedKey, alice.publicKey, bob.secretKey))
      .toBe(key);
  });

  it('lets the wrapper re-open their own wrap', () => {
    // The box secret is symmetric, so a member who wraps for themselves can
    // read it back — which is how a rotation keeps its author in the channel.
    const key = generateChannelKey();
    const wrapped = wrapChannelKey(key, alice.publicKey, alice.secretKey);
    expect(unwrapChannelKey(wrapped!.nonce, wrapped!.wrappedKey, alice.publicKey, alice.secretKey))
      .toBe(key);
  });

  it('is opaque to anyone it was not wrapped for', () => {
    const key = generateChannelKey();
    const wrapped = wrapChannelKey(key, bob.publicKey, alice.secretKey);
    expect(
      unwrapChannelKey(wrapped!.nonce, wrapped!.wrappedKey, alice.publicKey, mallory.secretKey)
    ).toBeNull();
  });

  it('rejects a wrap whose sender key was substituted', () => {
    // The box authenticator covers the sender, so a server that swaps the
    // recorded sender key cannot make the wrap open under the new one.
    const key = generateChannelKey();
    const wrapped = wrapChannelKey(key, bob.publicKey, alice.secretKey);
    expect(
      unwrapChannelKey(wrapped!.nonce, wrapped!.wrappedKey, mallory.publicKey, bob.secretKey)
    ).toBeNull();
  });

  it('refuses to wrap anything that is not a channel key', () => {
    expect(wrapChannelKey(toBase64(new Uint8Array(16)), bob.publicKey, alice.secretKey)).toBeNull();
    expect(wrapChannelKey('not base64 at all!!', bob.publicKey, alice.secretKey)).toBeNull();
  });

  it('refuses to accept an unwrapped payload of the wrong length', () => {
    // A "wrap" that opens to something other than a 32-byte key is not one; it
    // must not become a channel key that silently fails to decrypt anything.
    const wrapped = wrapChannelKey(generateChannelKey(), bob.publicKey, alice.secretKey);
    const tampered = toBase64(
      nacl.box(
        new Uint8Array(8),
        fromBase64(wrapped!.nonce),
        nacl.box.keyPair().publicKey,
        nacl.box.keyPair().secretKey
      )
    );
    expect(unwrapChannelKey(wrapped!.nonce, tampered, alice.publicKey, bob.secretKey)).toBeNull();
  });
});

describe('encryptChannelMessage / decryptChannelMessage', () => {
  const key = generateChannelKey();

  it('round-trips the whole payload, not just the text', () => {
    const payload = {
      text: 'the plan is off',
      attachment: { url: 'https://example.test/f', name: 'plan.pdf', size: 12 },
      replyText: 'what is the plan?'
    };
    const sealed = encryptChannelMessage(payload, 3, key);
    expect(sealed?.epoch).toBe(3);
    expect(decryptChannelMessage(sealed!, key)).toEqual(payload);
  });

  it('survives a round trip of non-ASCII text', () => {
    const text = 'héllo 🌍 — mit Ümlauten';
    const sealed = encryptChannelMessage({ text }, 1, key);
    expect(decryptChannelMessage(sealed!, key)?.text).toBe(text);
  });

  it('is unreadable under a different epoch key', () => {
    // This is what a rotation buys: the removed member still holds the old
    // key, and it opens nothing sent afterwards.
    const sealed = encryptChannelMessage({ text: 'after you left' }, 2, key);
    expect(decryptChannelMessage(sealed!, generateChannelKey())).toBeNull();
  });

  it('rejects a tampered ciphertext rather than returning partial text', () => {
    const sealed = encryptChannelMessage({ text: 'authentic' }, 1, key)!;
    const bytes = fromBase64(sealed.ciphertext);
    bytes[bytes.length - 1] ^= 0xff;
    expect(decryptChannelMessage({ ...sealed, ciphertext: toBase64(bytes) }, key)).toBeNull();
  });

  it('treats a plaintext that is not an object as undecryptable', () => {
    // Nothing this module writes is a bare string or an array, so a payload
    // that opens to one is not ours and must not be spread onto a message.
    const raw = fromBase64(key);
    const nonce = nacl.randomBytes(nacl.secretbox.nonceLength);
    const box = nacl.secretbox(new TextEncoder().encode('["nope"]'), nonce, raw);
    expect(
      decryptChannelMessage(
        { epoch: 1, nonce: toBase64(nonce), ciphertext: toBase64(box) },
        key
      )
    ).toBeNull();
  });

  it('refuses a key of the wrong length in either direction', () => {
    const short = toBase64(new Uint8Array(16));
    expect(encryptChannelMessage({ text: 'x' }, 1, short)).toBeNull();
    const sealed = encryptChannelMessage({ text: 'x' }, 1, key)!;
    expect(decryptChannelMessage(sealed, short)).toBeNull();
  });
});

describe('parseSealedMessage', () => {
  it('accepts a well-formed envelope', () => {
    expect(parseSealedMessage({ epoch: 2, nonce: 'n', ciphertext: 'c' })).toEqual({
      epoch: 2,
      nonce: 'n',
      ciphertext: 'c'
    });
  });

  it('rejects anything a server could send that is not one', () => {
    // Server frames are untrusted input: a half-formed envelope has to read as
    // "no envelope", never as one with missing pieces.
    expect(parseSealedMessage(undefined)).toBeNull();
    expect(parseSealedMessage(null)).toBeNull();
    expect(parseSealedMessage('enc')).toBeNull();
    expect(parseSealedMessage({ nonce: 'n', ciphertext: 'c' })).toBeNull();
    expect(parseSealedMessage({ epoch: 0, nonce: 'n', ciphertext: 'c' })).toBeNull();
    expect(parseSealedMessage({ epoch: 1.5, nonce: 'n', ciphertext: 'c' })).toBeNull();
    expect(parseSealedMessage({ epoch: 1, nonce: 5, ciphertext: 'c' })).toBeNull();
    expect(parseSealedMessage({ epoch: 1, nonce: 'n' })).toBeNull();
  });
});

describe('channelKeyFingerprint', () => {
  it('is stable for a key and different across keys', () => {
    const key = generateChannelKey();
    expect(channelKeyFingerprint(key)).toBe(channelKeyFingerprint(key));
    expect(channelKeyFingerprint(key)).not.toBe(channelKeyFingerprint(generateChannelKey()));
  });

  it('reads as grouped hex so two members can compare it out loud', () => {
    expect(channelKeyFingerprint(generateChannelKey())).toMatch(
      /^[0-9A-F]{4}( [0-9A-F]{4}){3}$/
    );
  });

  it('returns nothing rather than throwing on a malformed key', () => {
    expect(channelKeyFingerprint('not base64!!')).toBe('');
  });
});
