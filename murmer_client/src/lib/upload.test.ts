import { describe, expect, it } from 'vitest';
import nacl from 'tweetnacl';
import { fromBase64, loadKeyPair } from './keypair';
import { uploadForm, uploadErrorMessage } from './upload';

describe('uploadForm', () => {
  it('signs upload:<timestamp> with the stored identity key', () => {
    const before = Date.now();
    const form = uploadForm(new Blob(['data']), 'cat.png');
    const after = Date.now();

    const publicKey = form.get('publicKey') as string;
    const timestamp = form.get('timestamp') as string;
    const signature = form.get('signature') as string;

    expect(publicKey).toBe(loadKeyPair().publicKey);
    expect(Number(timestamp)).toBeGreaterThanOrEqual(before);
    expect(Number(timestamp)).toBeLessThanOrEqual(after);

    // The server verifies this exact message. Signing the bare timestamp
    // instead — what a presence frame signs — would make the two proofs
    // interchangeable and every upload fail.
    expect(
      nacl.sign.detached.verify(
        new TextEncoder().encode(`upload:${timestamp}`),
        fromBase64(signature),
        fromBase64(publicKey)
      )
    ).toBe(true);
  });

  it('puts the credentials ahead of the file', () => {
    const form = uploadForm(new Blob(['data']), 'cat.png');

    // The server authenticates as soon as it reaches the file part, so a file
    // sent first would be rejected however valid the credentials behind it.
    expect([...form.keys()]).toEqual(['publicKey', 'timestamp', 'signature', 'file']);
  });

  it('keeps the filename the server classifies the upload by', () => {
    const named = uploadForm(new Blob(['data']), 'cat.png').get('file') as File;
    expect(named.name).toBe('cat.png');

    // A File already carries its name; passing it through unchanged is what
    // keeps the extension safe-list looking at the real one.
    const file = new File(['data'], 'notes.pdf');
    expect((uploadForm(file).get('file') as File).name).toBe('notes.pdf');
  });
});

describe('uploadErrorMessage', () => {
  it('explains the statuses the endpoint answers with', () => {
    expect(uploadErrorMessage(401)).toMatch(/rejected/i);
    expect(uploadErrorMessage(403)).toMatch(/rejected/i);
    expect(uploadErrorMessage(429)).toMatch(/too quickly/i);
    expect(uploadErrorMessage(413, 'image')).toBe('That image is too large to upload.');
    expect(uploadErrorMessage(415, 'sound')).toBe('This sound type is not allowed on the server.');
  });

  it('leaves other statuses to the caller', () => {
    expect(uploadErrorMessage(200)).toBeNull();
    expect(uploadErrorMessage(500)).toBeNull();
  });
});
