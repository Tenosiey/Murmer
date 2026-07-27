/**
 * Shared image upload against a server's `/upload` endpoint.
 *
 * The endpoint validates type, size and magic bytes itself and every consumer
 * (avatar, profile, server icon, role icon) re-registers the returned URL over
 * the authenticated WebSocket, where the server checks it again. The size
 * check here only avoids a pointless round trip, and the error text exists so
 * callers can show something better than "failed".
 */

/** Human-readable form of a byte cap, for the size-limit message. */
function formatLimit(bytes: number): string {
  return bytes >= 1024 * 1024 ? `${bytes / (1024 * 1024)} MB` : `${bytes / 1024} KB`;
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
  const form = new FormData();
  form.append('file', file);
  try {
    const res = await fetch(httpBase + '/upload', { method: 'POST', body: form });
    if (res.status === 415) {
      return { ok: false, message: 'This image type is not allowed on the server.' };
    }
    if (res.status === 413) {
      return { ok: false, message: 'That image is too large to upload.' };
    }
    if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
    const data = await res.json();
    if (typeof data.url !== 'string') throw new Error('upload response missing url');
    return { ok: true, url: data.url };
  } catch (e) {
    console.error('image upload failed', e);
    return { ok: false, message: 'Upload failed. Please try again.' };
  }
}
