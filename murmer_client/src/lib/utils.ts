/**
 * Convert user input into a valid WebSocket URL.
 *
 * Accepts plain hostnames, HTTP URLs or WS URLs and ensures the
 * returned string begins with `ws://` or `wss://` and ends with `/ws`.
 */
export function normalizeServerUrl(input: string): string {
  let url = input.trim();
  if (!/^wss?:\/\//.test(url)) {
    if (/^https?:\/\//.test(url)) {
      url = url.replace(/^http/, 'ws');
      if (!/\/ws$/.test(url)) {
        url = url.replace(/\/?$/, '/ws');
      }
    } else {
      url = `ws://${url.replace(/\/$/, '')}/ws`;
    }
  }
  return url;
}
