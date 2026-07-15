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
