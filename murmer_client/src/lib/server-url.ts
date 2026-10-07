/**
 * Convert a stored server WebSocket URL (e.g. `ws://host:3001/ws`) into the
 * HTTP API base (`http://host:3001`, no trailing slash) used for uploads and
 * file downloads.
 */
export function httpBaseFromWs(wsUrl: string): string {
  const u = new URL(wsUrl);
  u.protocol = u.protocol.replace('ws', 'http');
  if (u.pathname.endsWith('/ws')) u.pathname = u.pathname.slice(0, -3);
  return u.toString().replace(/\/$/, '');
}

/**
 * `url` resolved to an absolute URL when it names a file stored on the server
 * at `httpBase` (`<httpBase>/files/…`), otherwise null.
 *
 * Message images go through this before they are rendered. The `image` field
 * is whatever the sender put there — inside an encrypted channel the server
 * cannot even see it — and any other host would learn the IP address of
 * every member who scrolls past the message.
 */
export function serverFileUrl(url: unknown, httpBase: string): string | null {
  if (typeof url !== 'string' || !httpBase) return null;
  let resolved: string;
  try {
    resolved = new URL(url, httpBase + '/').href;
  } catch {
    return null;
  }
  return resolved.startsWith(httpBase + '/files/') ? resolved : null;
}
