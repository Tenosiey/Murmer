import nacl from 'tweetnacl';
import { browser } from '$app/environment';

const STORAGE_KEY = 'murmer_keypair';

function toBase64(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes));
}

function fromBase64(s: string): Uint8Array {
  const b = atob(s);
  return Uint8Array.from([...b].map(c => c.charCodeAt(0)));
}

export interface KeyPair {
  publicKey: string;
  secretKey: string;
}

export function loadKeyPair(): KeyPair {
  if (browser) {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) return JSON.parse(stored);
    const kp = nacl.sign.keyPair();
    const pair = { publicKey: toBase64(kp.publicKey), secretKey: toBase64(kp.secretKey) };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(pair));
    return pair;
  }
  const kp = nacl.sign.keyPair();
  return { publicKey: toBase64(kp.publicKey), secretKey: toBase64(kp.secretKey) };
}

export function sign(text: string, secret: string): string {
  const msg = new TextEncoder().encode(text);
  const sig = nacl.sign.detached(msg, fromBase64(secret));
  return toBase64(sig);
}
