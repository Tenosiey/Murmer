import { normalizeServerUrl } from '$lib/utils';
import type { ServerEntry } from '$lib/stores/servers';

export interface InviteData {
  url: string;
  name?: string;
  password?: string;
}

/**
 * Generate a murmer:// invite link for a server entry.
 *
 * The link encodes the server URL, display name and optional password
 * so that other users can add the server without re-entering details.
 */
export function createInviteLink(server: ServerEntry): string {
  const params = new URLSearchParams({ url: server.url });
  if (server.name && server.name !== server.url) {
    params.set('name', server.name);
  }
  if (server.password) {
    params.set('password', server.password);
  }
  return `murmer://invite?${params.toString()}`;
}

/**
 * Parse a murmer:// invite link into its constituent server details.
 */
export function parseInviteLink(link: string): InviteData | null {
  try {
    const trimmed = link.trim();
    if (!trimmed) return null;

    const url = new URL(trimmed);
    if (url.protocol !== 'murmer:') return null;

    const action = url.hostname || url.pathname.replace(/^\//, '');
    if (action !== 'invite') return null;

    const serverUrl = url.searchParams.get('url');
    if (!serverUrl) return null;

    const data: InviteData = {
      url: normalizeServerUrl(serverUrl)
    };

    const name = url.searchParams.get('name')?.trim();
    if (name) data.name = name;

    const password = url.searchParams.get('password');
    if (password) data.password = password;

    return data;
  } catch (err) {
    if (import.meta.env.DEV) {
      console.error('Failed to parse invite link', err);
    }
    return null;
  }
}

