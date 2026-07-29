import nacl from 'tweetnacl';
import { browser } from '$app/environment';

const STORAGE_KEY = 'murmer_keypair';

export function toBase64(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes));
}

export function fromBase64(s: string): Uint8Array {
  const b = atob(s);
  return Uint8Array.from([...b].map(c => c.charCodeAt(0)));
}

export interface KeyPair {
  publicKey: string;
  secretKey: string;
}

/** Bytes of the seed an Ed25519 key is generated from. */
export const SEED_LENGTH = 32;

/**
 * The seed a key pair was derived from.
 *
 * An Ed25519 secret key *is* `seed || publicKey`, so this is a slice rather
 * than something that has to be stored alongside — which is why identities
 * created before there was any export path can still be exported and written
 * down. Nothing about the stored format changed to make backups possible.
 */
export function seedOf(pair: KeyPair): Uint8Array {
  const secret = fromBase64(pair.secretKey);
  if (secret.length !== nacl.sign.secretKeyLength) {
    throw new Error('Stored secret key has the wrong length');
  }
  return secret.slice(0, SEED_LENGTH);
}

/** The key pair a seed produces. Deterministic: the same seed is the same identity. */
export function keyPairFromSeed(seed: Uint8Array): KeyPair {
  if (seed.length !== SEED_LENGTH) {
    throw new Error(`Seed must be ${SEED_LENGTH} bytes, got ${seed.length}`);
  }
  const kp = nacl.sign.keyPair.fromSeed(seed);
  return { publicKey: toBase64(kp.publicKey), secretKey: toBase64(kp.secretKey) };
}

function generateKeyPair(): KeyPair {
  return keyPairFromSeed(nacl.randomBytes(SEED_LENGTH));
}

export function loadKeyPair(): KeyPair {
  if (browser) {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) return JSON.parse(stored);
    const pair = generateKeyPair();
    localStorage.setItem(STORAGE_KEY, JSON.stringify(pair));
    return pair;
  }
  return generateKeyPair();
}

/**
 * Replace the stored identity with the one a seed produces.
 *
 * Destructive in a way no other call in this module is: the previous key is
 * the only thing that can authenticate as its account on every server it was
 * used with, and the only thing that can decrypt the DMs addressed to it. The
 * caller is responsible for having warned the user.
 */
export function replaceKeyPair(seed: Uint8Array): KeyPair {
  const pair = keyPairFromSeed(seed);
  if (browser) localStorage.setItem(STORAGE_KEY, JSON.stringify(pair));
  return pair;
}

export function sign(text: string, secret: string): string {
  const msg = new TextEncoder().encode(text);
  const sig = nacl.sign.detached(msg, fromBase64(secret));
  return toBase64(sig);
}
