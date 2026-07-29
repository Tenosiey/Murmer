/*
  Backup and restore of the account identity.

  The Ed25519 key in `keypair.ts` is the account: the server binds a name to
  the first key that claims it and refuses every other one, and the same key
  derives the X25519 keys that DMs are encrypted to. Losing it therefore does
  not lose a session — it loses the account on every server at once, along with
  the ability to read a single DM ever received. There was no way to copy it to
  a second machine and no way to back it up.

  Two formats, the same 32-byte seed underneath:

  - A **recovery file**, encrypted with a passphrase. Carries the account name
    too, because a key without the name it claimed is only half an identity.
  - A **recovery phrase** of 24 BIP39 words. Weaker in what it carries (seed
    only) and unencrypted, but it is the one format that survives a dead disk
    with no backup, and a checksum catches a mistyped word before it silently
    produces the wrong identity.

  The file is deliberately not tied to this app's storage layout: it is plain
  JSON with a version, so a future format can be recognised rather than
  mis-parsed. Everything here is pure — no `localStorage`, no stores — so the
  round trip is testable without a browser.
*/
import nacl from 'tweetnacl';
import { entropyToMnemonic, mnemonicToEntropy, validateMnemonic } from '@scure/bip39';
// v2 of the package exports the wordlists with their `.js` suffix only.
import { wordlist } from '@scure/bip39/wordlists/english.js';
import { fromBase64, toBase64, SEED_LENGTH } from './keypair';

/** What a backup restores: the identity itself, and the name it goes by. */
export interface Identity {
  seed: Uint8Array;
  /** The account name in use when the backup was taken; empty if none. */
  user: string;
}

/** Marks a file as ours before a passphrase is even asked for. */
const FILE_FORMAT = 'murmer-identity';
const FILE_VERSION = 1;

/**
 * PBKDF2 rounds. Chosen against OWASP's 2023 floor for PBKDF2-HMAC-SHA256
 * (600k) rather than tuned to this machine: the file is decrypted on hardware
 * we know nothing about, possibly years from now, and a weak passphrase is the
 * realistic attack. Costs roughly a quarter second in a desktop webview.
 */
const PBKDF2_ITERATIONS = 600_000;
const SALT_LENGTH = 16;

export const FILE_EXTENSION = '.murmer-identity';

/** Suggested file name for a backup; scoped by account so several can coexist. */
export function suggestedFileName(user: string): string {
  const safe = user.replace(/[^a-zA-Z0-9._-]/g, '') || 'account';
  return `murmer-${safe}${FILE_EXTENSION}`;
}

/** Thrown for every failure a user can actually act on. */
export class IdentityError extends Error {}

/** Derive the file encryption key from a passphrase and its salt. */
async function deriveKey(passphrase: string, salt: Uint8Array): Promise<Uint8Array> {
  const material = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(passphrase),
    'PBKDF2',
    false,
    ['deriveBits']
  );
  const bits = await crypto.subtle.deriveBits(
    {
      name: 'PBKDF2',
      // `salt` is a plain Uint8Array; Web Crypto wants a BufferSource, and a
      // subarray's underlying buffer may be larger than the view.
      salt: salt.slice() as unknown as BufferSource,
      iterations: PBKDF2_ITERATIONS,
      hash: 'SHA-256'
    },
    material,
    nacl.secretbox.keyLength * 8
  );
  return new Uint8Array(bits);
}

/**
 * Encrypt an identity into the text of a recovery file.
 *
 * Rejects an empty passphrase rather than writing a file whose protection is
 * a formality — the file holds the account outright.
 */
export async function exportIdentityFile(
  identity: Identity,
  passphrase: string
): Promise<string> {
  if (!passphrase) throw new IdentityError('A passphrase is required.');
  if (identity.seed.length !== SEED_LENGTH) {
    throw new IdentityError('Refusing to export a malformed identity.');
  }

  const salt = nacl.randomBytes(SALT_LENGTH);
  const nonce = nacl.randomBytes(nacl.secretbox.nonceLength);
  const key = await deriveKey(passphrase, salt);
  const plaintext = new TextEncoder().encode(
    JSON.stringify({ seed: toBase64(identity.seed), user: identity.user })
  );

  return `${JSON.stringify(
    {
      format: FILE_FORMAT,
      version: FILE_VERSION,
      kdf: { name: 'PBKDF2-SHA256', iterations: PBKDF2_ITERATIONS, salt: toBase64(salt) },
      nonce: toBase64(nonce),
      ciphertext: toBase64(nacl.secretbox(plaintext, nonce, key))
    },
    null,
    2
  )}\n`;
}

/**
 * Decrypt a recovery file.
 *
 * Every failure is reported as something the user can act on. A wrong
 * passphrase and a corrupted file are indistinguishable to the cipher — both
 * are just a failed authenticator — so they share one message rather than
 * pretending to a certainty this cannot have.
 */
export async function importIdentityFile(text: string, passphrase: string): Promise<Identity> {
  let envelope: Record<string, unknown>;
  try {
    envelope = JSON.parse(text);
  } catch {
    throw new IdentityError('That file is not a Murmer recovery file.');
  }

  if (envelope?.format !== FILE_FORMAT) {
    throw new IdentityError('That file is not a Murmer recovery file.');
  }
  if (envelope.version !== FILE_VERSION) {
    throw new IdentityError(
      `This recovery file was written by a different version of Murmer (format ${String(
        envelope.version
      )}).`
    );
  }

  const kdf = envelope.kdf as { iterations?: unknown; salt?: unknown } | undefined;
  const iterations = kdf?.iterations;
  if (
    typeof kdf?.salt !== 'string' ||
    typeof iterations !== 'number' ||
    !Number.isInteger(iterations) ||
    iterations < 1 ||
    // A file claiming an absurd work factor would hang the app on open; the
    // ceiling is well above anything we write.
    iterations > 10_000_000 ||
    typeof envelope.nonce !== 'string' ||
    typeof envelope.ciphertext !== 'string'
  ) {
    throw new IdentityError('This recovery file is incomplete or damaged.');
  }

  let opened: Uint8Array | null;
  try {
    const key = await deriveKey(passphrase, fromBase64(kdf.salt));
    opened = nacl.secretbox.open(
      fromBase64(envelope.ciphertext),
      fromBase64(envelope.nonce),
      key
    );
  } catch {
    throw new IdentityError('This recovery file is incomplete or damaged.');
  }
  if (!opened) {
    throw new IdentityError('Wrong passphrase, or the file has been damaged.');
  }

  let payload: { seed?: unknown; user?: unknown };
  try {
    payload = JSON.parse(new TextDecoder().decode(opened));
  } catch {
    throw new IdentityError('This recovery file is incomplete or damaged.');
  }
  if (typeof payload.seed !== 'string') {
    throw new IdentityError('This recovery file is incomplete or damaged.');
  }

  const seed = fromBase64(payload.seed);
  if (seed.length !== SEED_LENGTH) {
    throw new IdentityError('This recovery file does not contain a usable key.');
  }
  return { seed, user: typeof payload.user === 'string' ? payload.user : '' };
}

/**
 * The 24-word recovery phrase for a seed.
 *
 * BIP39 only for its wordlist and checksum — no wallet derivation is involved.
 * The words encode the seed directly, so the phrase alone rebuilds the same
 * identity.
 */
export function seedToPhrase(seed: Uint8Array): string {
  if (seed.length !== SEED_LENGTH) {
    throw new IdentityError('Refusing to export a malformed identity.');
  }
  return entropyToMnemonic(seed, wordlist);
}

/**
 * The seed a recovery phrase encodes.
 *
 * Whitespace and capitalisation are forgiven because the phrase is typed off
 * paper; a word that is not in the list, or a phrase whose checksum fails, is
 * not — silently accepting either would hand back a *valid but different*
 * identity, and the user would find out when the server rejected their name.
 */
export function phraseToSeed(phrase: string): Uint8Array {
  const normalized = phrase.trim().toLowerCase().split(/\s+/).join(' ');
  if (!normalized) throw new IdentityError('Enter your 24-word recovery phrase.');

  const words = normalized.split(' ');
  if (words.length !== 24) {
    throw new IdentityError(`A recovery phrase is 24 words; this one has ${words.length}.`);
  }

  const unknown = words.filter((word) => !wordlist.includes(word));
  if (unknown.length > 0) {
    throw new IdentityError(
      `Not part of a recovery phrase: ${[...new Set(unknown)].slice(0, 3).join(', ')}`
    );
  }
  if (!validateMnemonic(normalized, wordlist)) {
    throw new IdentityError(
      'That phrase does not check out — a word is probably in the wrong place or mistyped.'
    );
  }

  const seed = mnemonicToEntropy(normalized, wordlist);
  if (seed.length !== SEED_LENGTH) {
    throw new IdentityError('That phrase does not encode a usable key.');
  }
  return seed;
}
