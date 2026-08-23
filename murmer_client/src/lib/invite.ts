import { normalizeServerUrl } from '$lib/utils';
import { httpBaseFromWs } from '$lib/server-url';
import type { ServerEntry } from '$lib/stores/servers';

export interface InviteData {
  url: string;
  name?: string;
  password?: string;
}

/** Path the invite landing route (`src/routes/invite/`) is served under. */
const INVITE_PATH = '/invite';

/**
 * Build a shareable invite link for a server entry.
 *
 * The link is an ordinary `https://` URL so that clicking it opens the web
 * client, and pasting it into the desktop client's "add server" field is
 * parsed by `parseInviteLink` — one link works for both.
 *
 * The server details live in the URL **fragment**, not the query string: a
 * fragment is never sent to the web server, so an invite carrying a server
 * password does not end up in access logs, proxy logs or a `Referer` header.
 *
 * `appOrigin` is where the web client is hosted (`location.origin` when we are
 * one). Without it the link points at the server's own origin, which is where
 * a server that hosts the web client itself serves it from.
 */
export function createInviteLink(server: ServerEntry, appOrigin?: string): string {
  const params = new URLSearchParams({ url: server.url });
  if (server.name && server.name !== server.url) {
    params.set('name', server.name);
  }
  if (server.password) {
    params.set('password', server.password);
  }
  const base = appOrigin?.replace(/\/$/, '') || httpBaseFromWs(server.url);
  return `${base}${INVITE_PATH}#${params.toString()}`;
}

function isInviteUrl(url: URL): boolean {
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return false;
  return url.pathname.replace(/\/$/, '') === INVITE_PATH;
}

/**
 * Whether a string is addressed at the invite route, whether or not it carries
 * usable details. Lets the "add server" field tell a broken invite from a
 * server address instead of storing `https://host/invite` as a hostname.
 */
export function looksLikeInviteLink(input: string): boolean {
  try {
    return isInviteUrl(new URL(input.trim()));
  } catch {
    return false;
  }
}

/**
 * Parse an invite link into its constituent server details.
 *
 * Accepts whatever the user pasted or the browser was opened with, so it is
 * strict about what it takes: only `http`/`https` links whose path is the
 * invite route, and only after the server URL has been normalized the same
 * way the "add server" field normalizes typed input.
 */
export function parseInviteLink(link: string): InviteData | null {
  try {
    const trimmed = link.trim();
    if (!trimmed) return null;

    const url = new URL(trimmed);
    if (!isInviteUrl(url)) return null;

    // Fragment first (that is where `createInviteLink` puts them); a
    // hand-written link may use a plain query string instead.
    const fragment = new URLSearchParams(url.hash.replace(/^#/, ''));
    const params = fragment.has('url') ? fragment : url.searchParams;

    const serverUrl = params.get('url');
    if (!serverUrl?.trim()) return null;

    const data: InviteData = {
      url: normalizeServerUrl(serverUrl)
    };

    const name = params.get('name')?.trim();
    if (name) data.name = name;

    const password = params.get('password');
    if (password) data.password = password;

    return data;
  } catch (err) {
    if (import.meta.env.DEV) {
      console.error('Failed to parse invite link', err);
    }
    return null;
  }
}
