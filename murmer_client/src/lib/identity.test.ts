import { describe, expect, it } from 'vitest';
import nacl from 'tweetnacl';
import {
  IdentityError,
  exportIdentityFile,
  importIdentityFile,
  phraseToSeed,
  seedToPhrase,
  suggestedFileName
} from './identity';
import { keyPairFromSeed, seedOf, toBase64, SEED_LENGTH } from './keypair';

/** A fixed seed, so a failure names the same identity every run. */
const SEED = new Uint8Array(SEED_LENGTH).map((_, i) => (i * 7 + 3) & 0xff);

/**
 * A recovery file assembled from the *documented* format rather than by
 * calling the exporter, so the two can be compared against each other.
 */
async function handBuiltFile(payload: unknown, passphrase: string): Promise<string> {
  const salt = nacl.randomBytes(16);
  const nonce = nacl.randomBytes(nacl.secretbox.nonceLength);
  const material = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(passphrase),
    'PBKDF2',
    false,
    ['deriveBits']
  );
  const bits = await crypto.subtle.deriveBits(
    { name: 'PBKDF2', salt: salt.slice() as unknown as BufferSource, iterations: 600_000, hash: 'SHA-256' },
    material,
    256
  );
  const box = nacl.secretbox(
    new TextEncoder().encode(JSON.stringify(payload)),
    nonce,
    new Uint8Array(bits)
  );
  return JSON.stringify({
    format: 'murmer-identity',
    version: 1,
    kdf: { name: 'PBKDF2-SHA256', iterations: 600_000, salt: toBase64(salt) },
    nonce: toBase64(nonce),
    ciphertext: toBase64(box)
  });
}

describe('recovery file', () => {
  it('round-trips the identity and the account name', async () => {
    const file = await exportIdentityFile({ seed: SEED, user: 'alice' }, 'correct horse');

    const restored = await importIdentityFile(file, 'correct horse');

    expect(restored.seed).toEqual(SEED);
    expect(restored.user).toBe('alice');
  });

  it('restores the same key pair, which is the whole point', async () => {
    const file = await exportIdentityFile({ seed: SEED, user: 'alice' }, 'pw');

    const restored = await importIdentityFile(file, 'pw');

    // Same public key means the server still recognises the account, and the
    // same secret means DM history stays readable.
    expect(keyPairFromSeed(restored.seed)).toEqual(keyPairFromSeed(SEED));
  });

  it('rejects the wrong passphrase without hinting at how close it was', async () => {
    const file = await exportIdentityFile({ seed: SEED, user: 'alice' }, 'correct horse');

    await expect(importIdentityFile(file, 'correct hors')).rejects.toThrow(
      /Wrong passphrase, or the file has been damaged/
    );
  });

  it('refuses to write a file with no passphrase at all', async () => {
    await expect(exportIdentityFile({ seed: SEED, user: 'alice' }, '')).rejects.toThrow(
      IdentityError
    );
  });

  it('never stores the seed in the clear', async () => {
    const file = await exportIdentityFile({ seed: SEED, user: 'alice' }, 'pw');

    expect(file).not.toContain(toBase64(SEED));
    // The account name is inside the box too — a backup should not announce
    // whose account it opens.
    expect(file).not.toContain('alice');
  });

  it('produces a different file every time, so two backups cannot be compared', async () => {
    const first = await exportIdentityFile({ seed: SEED, user: 'alice' }, 'pw');
    const second = await exportIdentityFile({ seed: SEED, user: 'alice' }, 'pw');

    expect(first).not.toBe(second);
    // ...and both still open.
    expect((await importIdentityFile(second, 'pw')).seed).toEqual(SEED);
  });

  it('detects a tampered ciphertext rather than returning a wrong key', async () => {
    const envelope = JSON.parse(await exportIdentityFile({ seed: SEED, user: 'alice' }, 'pw'));
    const bytes = [...atob(envelope.ciphertext)].map((c) => c.charCodeAt(0));
    bytes[0] ^= 0xff;
    envelope.ciphertext = btoa(String.fromCharCode(...bytes));

    await expect(importIdentityFile(JSON.stringify(envelope), 'pw')).rejects.toThrow(
      IdentityError
    );
  });

  it('names what is wrong with a file that is not one of ours', async () => {
    await expect(importIdentityFile('not json at all', 'pw')).rejects.toThrow(
      /not a Murmer recovery file/
    );
    await expect(importIdentityFile('{"format":"something-else"}', 'pw')).rejects.toThrow(
      /not a Murmer recovery file/
    );
  });

  it('reports a future format instead of failing to parse it', async () => {
    const envelope = JSON.parse(await exportIdentityFile({ seed: SEED, user: 'alice' }, 'pw'));
    envelope.version = 2;

    await expect(importIdentityFile(JSON.stringify(envelope), 'pw')).rejects.toThrow(
      /different version of Murmer/
    );
  });

  it('refuses an absurd work factor instead of hanging on it', async () => {
    const envelope = JSON.parse(await exportIdentityFile({ seed: SEED, user: 'alice' }, 'pw'));
    envelope.kdf.iterations = 1e12;

    await expect(importIdentityFile(JSON.stringify(envelope), 'pw')).rejects.toThrow(
      /incomplete or damaged/
    );
  });

  it('rejects a well-formed file whose payload is not a usable key', async () => {
    // Built by hand, because the export path cannot produce this: a file that
    // decrypts cleanly and still does not hold an identity.
    const file = await handBuiltFile({ seed: toBase64(new Uint8Array(16)), user: 'alice' }, 'pw');

    await expect(importIdentityFile(file, 'pw')).rejects.toThrow(/does not contain a usable key/);
  });

  it('is readable by an independent implementation of the documented format', async () => {
    // The format is a promise to anyone holding an old backup, so it is
    // reconstructed here from its description rather than from the exporter:
    // PBKDF2-SHA256 at the stated iteration count over the stated salt, then
    // NaCl secretbox. If the exporter drifts from this, the file stops being
    // recoverable and only this test would notice.
    const file = await handBuiltFile({ seed: toBase64(SEED), user: 'alice' }, 'pw');

    const restored = await importIdentityFile(file, 'pw');

    expect(restored.seed).toEqual(SEED);
    expect(restored.user).toBe('alice');
  });
});

describe('recovery phrase', () => {
  it('round-trips a seed through 24 words', () => {
    const phrase = seedToPhrase(SEED);

    expect(phrase.split(' ')).toHaveLength(24);
    expect(phraseToSeed(phrase)).toEqual(SEED);
  });

  it('forgives the way a phrase is typed off paper', () => {
    const phrase = seedToPhrase(SEED);

    expect(phraseToSeed(`  ${phrase.toUpperCase().replace(/ /g, '   ')}\n`)).toEqual(SEED);
  });

  it('catches a mistyped word instead of producing a different identity', () => {
    const words = seedToPhrase(SEED).split(' ');
    words[3] = 'zzzznotaword';

    expect(() => phraseToSeed(words.join(' '))).toThrow(/Not part of a recovery phrase/);
  });

  it('catches two swapped words, which the checksum exists for', () => {
    const words = seedToPhrase(SEED).split(' ');
    [words[0], words[1]] = [words[1], words[0]];

    // Every word is real, so only the checksum can tell — and it must, or the
    // user restores a valid key that is not theirs and the server rejects the
    // name with no explanation.
    expect(() => phraseToSeed(words.join(' '))).toThrow(/does not check out/);
  });

  it('says how many words are missing', () => {
    const words = seedToPhrase(SEED).split(' ').slice(0, 12);

    expect(() => phraseToSeed(words.join(' '))).toThrow(/24 words; this one has 12/);
  });

  it('asks for the phrase rather than complaining about nothing', () => {
    expect(() => phraseToSeed('   ')).toThrow(/Enter your 24-word recovery phrase/);
  });

  it('agrees with the file format on what the identity is', async () => {
    const file = await exportIdentityFile({ seed: SEED, user: 'alice' }, 'pw');

    // Both formats must restore the same account, or a user with one backup
    // and not the other would find only one of them works.
    expect(phraseToSeed(seedToPhrase(SEED))).toEqual((await importIdentityFile(file, 'pw')).seed);
  });
});

describe('seedOf', () => {
  it('recovers the seed of a key pair generated before seeds were stored', () => {
    // What `nacl.sign.keyPair()` produced for every identity created before
    // this feature existed: the seed is the first half of the secret key, so
    // those identities are exportable without any migration.
    const legacy = nacl.sign.keyPair();
    const pair = { publicKey: toBase64(legacy.publicKey), secretKey: toBase64(legacy.secretKey) };

    expect(keyPairFromSeed(seedOf(pair))).toEqual(pair);
  });

  it('refuses a stored secret of the wrong length', () => {
    expect(() => seedOf({ publicKey: '', secretKey: toBase64(new Uint8Array(32)) })).toThrow();
  });
});

describe('suggestedFileName', () => {
  it('scopes the backup by account', () => {
    expect(suggestedFileName('alice')).toBe('murmer-alice.murmer-identity');
  });

  it('strips anything that would not survive a file system', () => {
    expect(suggestedFileName('../../etc/passwd')).toBe('murmer-....etcpasswd.murmer-identity');
    expect(suggestedFileName('')).toBe('murmer-account.murmer-identity');
  });
});
