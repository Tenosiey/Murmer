/**
 * Uploads against a server's `/upload` endpoint.
 *
 * The endpoint is authenticated: it only accepts the upload session of a
 * live, authenticated WebSocket connection to that server, so every caller
 * builds its multipart body with `uploadForm` rather than by hand. The
 * session travels as an ordinary form field ahead of the file — a custom
 * header would turn the request into a preflighted one, which servers running
 * the recommended configuration (CORS disabled) do not answer.
 *
 * Beyond that the endpoint validates type, size and magic bytes itself and
 * every consumer (chat attachment, avatar, profile, server icon, role icon)
 * re-registers the returned URL over the authenticated WebSocket, where the
 * server checks it again. The size check here only avoids a pointless round trip, and the error
 * text exists so callers can show something better than "failed".
 */

import { openFile, sealFile } from './channel-crypto';
import { t } from './i18n';

/**
 * The open connection's upload session: the challenge the server greeted it
 * with, which the server accepts on `/upload` once `presence` has signed it.
 * Set by the chat store; there is one connection at a time.
 */
let uploadSession: string | null = null;

export function setUploadSession(session: string | null): void {
  uploadSession = session;
}

/** Render a byte count the way the upload settings talk about it. */
export function formatUploadSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${Number.isInteger(mb) ? mb : mb.toFixed(1)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

/** Human-readable form of a byte cap, for the size-limit message. */
function formatLimit(bytes: number): string {
  return bytes >= 1024 * 1024 ? `${bytes / (1024 * 1024)} MB` : `${bytes / 1024} KB`;
}

/**
 * Build the multipart body for an upload: the session first, then the file.
 *
 * Order matters — the server checks the session as soon as it reaches the
 * file part, so that it never buffers bytes for a caller it has not
 * authenticated. `sealed` marks ciphertext from `sealFile`, which the server
 * then stores without a name and without looking for an image in it.
 */
export function uploadForm(file: Blob, filename?: string, sealed = false): FormData {
  const form = new FormData();
  form.append('session', uploadSession ?? '');
  if (sealed) form.append('sealed', '1');
  if (filename === undefined) form.append('file', file);
  else form.append('file', file, filename);
  return form;
}

/**
 * User-facing text for the statuses `/upload` answers with, or `null` when the
 * response is not one of them. `subject` names what was being uploaded, e.g.
 * "image" or "sound".
 */
export function uploadErrorMessage(
  status: number,
  subject: 'file' | 'image' | 'sound' = 'file'
): string | null {
  switch (status) {
    case 401:
    case 403:
      return t('upload.rejected');
    case 429:
      return t('upload.tooQuickly');
    case 413:
      return t(`upload.tooLarge.${subject}`);
    case 415:
      return t(`upload.typeNotAllowed.${subject}`);
    case 507:
      return t('upload.outOfSpace');
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
  if (!httpBase) return { ok: false, message: t('upload.connectFirst') };
  if (file.size > maxBytes) {
    return { ok: false, message: t('upload.imageTooLarge', { limit: formatLimit(maxBytes) }) };
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
    return { ok: false, message: t('upload.imageFailed') };
  }
}

/** What a chat message carries for an uploaded file. */
export type UploadedContent =
  | { image: string; imageKey?: string }
  | { attachment: { url: string; name: string; size: number; key?: string } };

export type AttachmentResult =
  | { ok: true; content: UploadedContent }
  | { ok: false; message: string };

/**
 * Upload a chat attachment to `httpBase`. Images come back as `{ image }`,
 * which renders inline; anything else as a download card named after what
 * the server stored. `maxBytes` is the server's current limit, named in the
 * message when the server refuses the size after all — the policy can change
 * between picking the file and sending it. Never throws: failures come back
 * as `{ ok: false, message }`.
 *
 * With `seal`, for an encrypted channel, the bytes are encrypted first and
 * the key comes back with the content, to travel inside the sealed message.
 * Only the extension goes to the server, which picks the upload category.
 */
export async function uploadAttachment(
  httpBase: string,
  file: File,
  maxBytes: number,
  seal = false
): Promise<AttachmentResult> {
  try {
    let body: FormData;
    let key: string | undefined;
    if (seal) {
      const sealed = sealFile(new Uint8Array(await file.arrayBuffer()));
      key = sealed.key;
      const extension = /\.[^.]+$/.exec(file.name)?.[0] ?? '';
      body = uploadForm(new Blob([sealed.data]), `sealed${extension}`, true);
    } else {
      body = uploadForm(file);
    }
    const res = await fetch(httpBase + '/upload', { method: 'POST', body });
    if (res.status === 413) {
      return {
        ok: false,
        message: t('upload.fileTooLarge', { limit: formatUploadSize(maxBytes) })
      };
    }
    const message = uploadErrorMessage(res.status);
    if (message) return { ok: false, message };
    if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
    const data = await res.json();
    if (typeof data.url !== 'string') throw new Error('upload response missing url');
    const url = data.url.startsWith('http') ? data.url : httpBase + data.url;
    if (key) {
      return file.type.startsWith('image/')
        ? { ok: true, content: { image: url, imageKey: key } }
        : { ok: true, content: { attachment: { url, name: file.name, size: file.size, key } } };
    }
    if (data.kind === 'image' || file.type.startsWith('image/')) {
      return { ok: true, content: { image: url } };
    }
    return {
      ok: true,
      content: {
        attachment: {
          url,
          name: typeof data.name === 'string' ? data.name : file.name,
          size: typeof data.size === 'number' ? data.size : file.size
        }
      }
    };
  } catch (e) {
    console.error('upload failed', e);
    return { ok: false, message: t('upload.fileFailed') };
  }
}

/**
 * Fetch a stored file as a blob, opening it first when it was sealed with
 * `key`. Throws when the fetch fails or the file does not open, which callers
 * show as a file that could not be loaded.
 */
export async function loadFile(url: string, key?: string): Promise<Blob> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  if (key === undefined) return response.blob();
  const opened = openFile(new Uint8Array(await response.arrayBuffer()), key);
  if (!opened) throw new Error('file does not open with its key');
  return new Blob([opened]);
}

/**
 * Save a sealed attachment: open it in memory and hand the plaintext to the
 * browser as a download. A link to the stored file would save ciphertext.
 */
export async function saveSealedFile(url: string, name: string, key: string): Promise<void> {
  try {
    const href = URL.createObjectURL(await loadFile(url, key));
    const anchor = document.createElement('a');
    anchor.href = href;
    anchor.download = name;
    anchor.click();
    // Revoked later rather than now: the download reads the blob after click.
    setTimeout(() => URL.revokeObjectURL(href), 60_000);
  } catch (error) {
    console.warn('Could not open sealed attachment', error);
  }
}
