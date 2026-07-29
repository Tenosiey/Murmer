import { describe, expect, it } from 'vitest';
import nacl from 'tweetnacl';
import {
  SEED_LENGTH,
  fromBase64,
  keyPairFromSeed,
  loadKeyPair,
  replaceKeyPair,
  seedOf,
  sign,
  toBase64
} from './keypair';

const STORAGE_KEY = 'murmer_keypair';
const SEED = new Uint8Array(SEED_LENGTH).map((_, i) => (i * 11 + 5) & 0xff);

describe('loadKeyPair', () => {
  it('keeps returning the identity it first generated', () => {
    const first = loadKeyPair();

    // The account is the key: a second call that generated a new one would
    // silently lock the user out of the name they already claimed.
    expect(loadKeyPair()).toEqual(first);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}')).toEqual(first);
  });

  it('generates a key whose seed is recoverable, so it can be backed up', () => {
    const pair = loadKeyPair();

    expect(keyPairFromSeed(seedOf(pair))).toEqual(pair);
  });

  it('signs with the key it stored', () => {
    const pair = loadKeyPair();
    const timestamp = '1785331000000';

    const ok = nacl.sign.detached.verify(
      new TextEncoder().encode(timestamp),
      fromBase64(sign(timestamp, pair.secretKey)),
      fromBase64(pair.publicKey)
    );

    // What the server checks on every presence frame.
    expect(ok).toBe(true);
  });
});

describe('keyPairFromSeed', () => {
  it('is deterministic — the same seed is the same account', () => {
    expect(keyPairFromSeed(SEED)).toEqual(keyPairFromSeed(SEED));
  });

  it('refuses a seed of the wrong length rather than deriving something else', () => {
    expect(() => keyPairFromSeed(new Uint8Array(16))).toThrow(/32 bytes/);
    expect(() => keyPairFromSeed(new Uint8Array(64))).toThrow(/32 bytes/);
  });
});

describe('replaceKeyPair', () => {
  it('installs the restored identity so the next load returns it', () => {
    loadKeyPair();

    const restored = replaceKeyPair(SEED);

    // This is what restoring a backup does; if it did not persist, the app
    // would come back as the old identity after the reload that follows.
    expect(restored).toEqual(keyPairFromSeed(SEED));
    expect(loadKeyPair()).toEqual(restored);
  });

  it('round-trips through the seed a backup would carry', () => {
    const original = loadKeyPair();
    const seed = seedOf(original);

    replaceKeyPair(new Uint8Array(SEED_LENGTH));
    expect(loadKeyPair()).not.toEqual(original);

    // Restoring a backup taken before the identity was replaced must bring the
    // original account back exactly, or DM history stays unreadable.
    expect(replaceKeyPair(seed)).toEqual(original);
  });

  it('refuses a malformed seed without touching the stored identity', () => {
    const original = loadKeyPair();

    expect(() => replaceKeyPair(new Uint8Array(31))).toThrow();
    expect(loadKeyPair()).toEqual(original);
  });
});

describe('base64 helpers', () => {
  it('round-trips the byte values a key can contain', () => {
    const bytes = new Uint8Array(256).map((_, i) => i);

    expect(fromBase64(toBase64(bytes))).toEqual(bytes);
  });
});
