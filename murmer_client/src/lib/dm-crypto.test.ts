import { describe, expect, it } from 'vitest';
import nacl from 'tweetnacl';
import { decryptDm, dmFingerprint, encryptDm } from './dm-crypto';
import { toBase64 } from './keypair';

/** A fresh Ed25519 identity, the same shape `keypair.ts` persists. */
function identity() {
  const pair = nacl.sign.keyPair();
  return { publicKey: toBase64(pair.publicKey), secretKey: toBase64(pair.secretKey) };
}

const alice = identity();
const bob = identity();
const mallory = identity();

describe('encryptDm / decryptDm', () => {
  it('lets the recipient read what the sender wrote', () => {
    const box = encryptDm('hello bob', bob.publicKey, alice.secretKey);
    expect(box).not.toBeNull();
    expect(decryptDm(box!.nonce, box!.ciphertext, alice.publicKey, bob.secretKey)).toBe(
      'hello bob'
    );
  });

  it('lets the sender re-read their own history', () => {
    // The box secret is symmetric, which is what makes a single stored
    // ciphertext readable by both participants.
    const box = encryptDm('hello bob', bob.publicKey, alice.secretKey);
    expect(decryptDm(box!.nonce, box!.ciphertext, bob.publicKey, alice.secretKey)).toBe(
      'hello bob'
    );
  });

  it('survives a round trip of non-ASCII text', () => {
    const text = 'héllo 🌍 — mit Ümlauten';
    const box = encryptDm(text, bob.publicKey, alice.secretKey);
    expect(decryptDm(box!.nonce, box!.ciphertext, alice.publicKey, bob.secretKey)).toBe(text);
  });

  it('is unreadable to a third party', () => {
    const box = encryptDm('for bob only', bob.publicKey, alice.secretKey);
    expect(decryptDm(box!.nonce, box!.ciphertext, alice.publicKey, mallory.secretKey)).toBeNull();
    expect(decryptDm(box!.nonce, box!.ciphertext, mallory.publicKey, bob.secretKey)).toBeNull();
  });

  it('refuses a tampered ciphertext or nonce', () => {
    const box = encryptDm('transfer 10', bob.publicKey, alice.secretKey)!;

    const bytes = [...atob(box.ciphertext)].map((c) => c.charCodeAt(0));
    bytes[0] ^= 0xff;
    const tampered = toBase64(Uint8Array.from(bytes));
    expect(decryptDm(box.nonce, tampered, alice.publicKey, bob.secretKey)).toBeNull();

    const otherNonce = toBase64(nacl.randomBytes(nacl.box.nonceLength));
    expect(decryptDm(otherNonce, box.ciphertext, alice.publicKey, bob.secretKey)).toBeNull();
  });

  it('uses a fresh nonce for every message', () => {
    const a = encryptDm('same text', bob.publicKey, alice.secretKey)!;
    const b = encryptDm('same text', bob.publicKey, alice.secretKey)!;
    expect(a.nonce).not.toBe(b.nonce);
    expect(a.ciphertext).not.toBe(b.ciphertext);
  });

  it('returns null rather than throwing on an unparseable key', () => {
    // A corrupted binding or a hostile server response must degrade to "cannot
    // encrypt", never to an exception in the middle of rendering.
    expect(encryptDm('hi', 'not base64!!', alice.secretKey)).toBeNull();
    expect(decryptDm('', '', 'not base64!!', alice.secretKey)).toBeNull();
    expect(encryptDm('hi', bob.publicKey, 'not base64!!')).toBeNull();
  });

  it('does not currently reject a well-formed but wrong-length peer key', () => {
    // `ed2curve.convertPublicKey` does not check its input length: it returns
    // a 32-byte X25519 key for a 0- or 5-byte Ed25519 "key" just as happily as
    // for a real one, so `dhKeys` never sees the null it looks for.
    //
    // This documents today's behaviour rather than endorsing it. The result is
    // a ciphertext nobody can open — the sender believes the DM went out while
    // the peer sees a permanent decrypt-failure placeholder. A length check in
    // `dhKeys` would turn it into an honest null. If that check is added, this
    // test is the one to delete.
    for (const key of ['', toBase64(new Uint8Array(5))]) {
      expect(encryptDm('hi', key, alice.secretKey)).not.toBeNull();
    }

    const box = encryptDm('hi', toBase64(new Uint8Array(5)), alice.secretKey)!;
    expect(decryptDm(box.nonce, box.ciphertext, bob.publicKey, alice.secretKey)).toBeNull();
  });

  it('returns null rather than throwing on a malformed payload', () => {
    expect(decryptDm('not base64!!', 'also bad', bob.publicKey, alice.secretKey)).toBeNull();
    expect(decryptDm('', '', bob.publicKey, alice.secretKey)).toBeNull();
  });
});

describe('dmFingerprint', () => {
  it('reads the same on both ends of a conversation', () => {
    // Each side passes its own key first, so an order-dependent digest would
    // make out-of-band comparison useless.
    expect(dmFingerprint(alice.publicKey, bob.publicKey)).toBe(
      dmFingerprint(bob.publicKey, alice.publicKey)
    );
  });

  it('differs for a different pair of keys', () => {
    expect(dmFingerprint(alice.publicKey, bob.publicKey)).not.toBe(
      dmFingerprint(alice.publicKey, mallory.publicKey)
    );
  });

  it('is grouped for reading aloud', () => {
    expect(dmFingerprint(alice.publicKey, bob.publicKey)).toMatch(
      /^[0-9A-F]{4}( [0-9A-F]{4}){3}$/
    );
  });
});
