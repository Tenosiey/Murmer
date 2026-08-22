/**
 * Uploads against a server's `/upload` endpoint.
 *
 * The endpoint is authenticated: it only accepts a fresh, single-use Ed25519
 * proof from a key that already has an account on that server, so every
 * caller builds its multipart body with `uploadForm` rather than by hand. The
 * proof travels as ordinary form fields ahead of the file — a custom header
 * would turn the request into a preflighted one, which servers running the
 * recommended configuration (CORS disabled) do not answer.
 *
 * Beyond that the endpoint validates type, size and magic bytes itself and
 * every consumer (avatar, profile, server icon, role icon) re-registers the
 * returned URL over the authenticated WebSocket, where the server checks it
 * again. The size check here only avoids a pointless round trip, and the error
 * text exists so callers can show something better than "failed".
 */

import { loadKeyPair, sign } from './keypair';

/** Human-readable form of a byte cap, for the size-limit message. */
function formatLimit(bytes: number): string {
  return bytes >= 1024 * 1024 ? `${bytes / (1024 * 1024)} MB` : `${bytes / 1024} KB`;
}

/**
 * Build the multipart body for an upload: the identity proof first, then the
 * file.
 *
 * Order matters — the server verifies the credentials as soon as it reaches
 * the file part, so that it never buffers bytes for a caller it has not
 * authenticated. The signature covers `upload:<timestamp>` rather than the
 * bare timestamp a presence frame signs, which keeps the two proofs from
 * being interchangeable.
 */
export function uploadForm(file: Blob, filename?: string): FormData {
  const pair = loadKeyPair();
  const timestamp = Date.now().toString();
  const form = new FormData();
  form.append('publicKey', pair.publicKey);
  form.append('timestamp', timestamp);
  form.append('signature', sign(`upload:${timestamp}`, pair.secretKey));
  if (filename === undefined) form.append('file', file);
  else form.append('file', file, filename);
  return form;
}

/**
 * User-facing text for the statuses `/upload` answers with, or `null` when the
 * response is not one of them. `subject` names what was being uploaded, e.g.
 * "image" or "sound".
 */
export function uploadErrorMessage(status: number, subject = 'file'): string | null {
  switch (status) {
    case 401:
    case 403:
      return 'The server rejected the upload. Reconnect and try again.';
    case 429:
      return 'You are uploading too quickly. Please wait a moment and try again.';
    case 413:
      return `That ${subject} is too large to upload.`;
    case 415:
      return `This ${subject} type is not allowed on the server.`;
    default:
      return null;
  }
}

export type UploadResult = { ok: true; url: string } | { ok: false; message: string };

/**
 * Upload `file` to `httpBase` and return the stored `/files/<key>` URL.
 * Never throws: failures come back as `{ ok: false, message }`.
 */
export async function uploadImage(
  httpBase: string,
  file: File,
  maxBytes: number
): Promise<UploadResult> {
  if (!httpBase) return { ok: false, message: 'Connect to a server first.' };
  if (file.size > maxBytes) {
    return { ok: false, message: `Images must be ${formatLimit(maxBytes)} or smaller.` };
  }
  try {
    const res = await fetch(httpBase + '/upload', {
      method: 'POST',
      body: uploadForm(file)
    });
    const message = uploadErrorMessage(res.status, 'image');
    if (message) return { ok: false, message };
    if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
    const data = await res.json();
    if (typeof data.url !== 'string') throw new Error('upload response missing url');
    return { ok: true, url: data.url };
  } catch (e) {
    console.error('image upload failed', e);
    return { ok: false, message: 'Upload failed. Please try again.' };
  }
}
