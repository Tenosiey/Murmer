/**
 * Convert user input into a valid WebSocket URL.
 *
 * Accepts plain hostnames, HTTP URLs or WS URLs and ensures the
 * returned string begins with `ws://` or `wss://` and ends with `/ws`.
 * A bare hostname gets `wss://` unless it is a local or LAN address: a
 * `ws://` guess would send messages, the server password and invite codes
 * across the internet in clear. A plaintext server has to be asked for
 * explicitly with `ws://` or `http://`.
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
      const bare = url.replace(/\/$/, '');
      const scheme = isLocalHost(hostOf(`ws://${bare}`)) ? 'ws' : 'wss';
      url = `${scheme}://${bare}/ws`;
    }
  }
  return url;
}

/** Hostname of a URL without IPv6 brackets, or '' when it does not parse. */
function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^\[|\]$/g, '');
  } catch {
    return '';
  }
}

/**
 * Whether `host` is this machine or a private-network address: loopback,
 * RFC 1918, link-local, IPv6 unique-local, an mDNS `.local` name or a single
 * label such as `homeserver`, which only resolves on a LAN.
 */
export function isLocalHost(host: string): boolean {
  const h = host.toLowerCase();
  if (h === 'localhost' || h.endsWith('.localhost') || h.endsWith('.local')) return true;
  if (!h.includes('.') && !h.includes(':')) return h !== '';
  const v4 = h.match(/^(\d+)\.(\d+)\.\d+\.\d+$/);
  if (v4) {
    const [a, b] = [Number(v4[1]), Number(v4[2])];
    return (
      a === 127 ||
      a === 10 ||
      (a === 172 && b >= 16 && b <= 31) ||
      (a === 192 && b === 168) ||
      (a === 169 && b === 254)
    );
  }
  return h === '::1' || /^f[cd][0-9a-f]{2}:/.test(h) || /^fe[89ab][0-9a-f]:/.test(h);
}

/**
 * Whether a stored server URL is plaintext `ws://` to a host outside the
 * local network — the case where the chat page warns "not encrypted".
 */
export function isUnencryptedRemote(url: string): boolean {
  return url.startsWith('ws://') && !isLocalHost(hostOf(url));
}
