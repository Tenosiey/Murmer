/**
 * Links to a single message ("copy link" in a message's actions).
 *
 * Shaped like an invite link — an ordinary `https://` URL with the details in
 * the fragment — so the same link works pasted into a browser (it opens the
 * web client's chat route) and clicked inside Murmer, where the chat page
 * catches it and jumps without leaving the app.
 *
 * A link only ever opens a server already in the user's list. Connecting is
 * what hands a server the user's name, key and address, so a link must never
 * be able to make that happen on its own.
 */
import { writable } from 'svelte/store';
import { normalizeServerUrl } from '$lib/utils';
import { httpBaseFromWs } from '$lib/server-url';

export interface MessageLink {
  /** The server's WebSocket URL, normalized like the server list's. */
  server: string;
  channel: number;
  message: number;
}

const CHAT_PATH = '/chat';

/**
 * Build a link to a message. `appOrigin` is where the web client is hosted
 * (`location.origin` when we are one); without it the link points at the
 * server's own origin, as invite links do.
 */
export function createMessageLink(link: MessageLink, appOrigin?: string): string {
  const params = new URLSearchParams({
    server: link.server,
    channel: String(link.channel),
    message: String(link.message)
  });
  const base = appOrigin?.replace(/\/$/, '') || httpBaseFromWs(link.server);
  return `${base}${CHAT_PATH}#${params.toString()}`;
}

function positiveId(value: string | null): number | null {
  if (!value || !/^\d{1,15}$/.test(value)) return null;
  const id = Number(value);
  return id > 0 ? id : null;
}

/**
 * Parse a message link, or return null for anything else. Any origin is
 * accepted, since a link points wherever the copying client was hosted; the
 * path and the three fragment fields are what make it one of ours.
 */
export function parseMessageLink(href: string): MessageLink | null {
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return null;
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
  if (url.pathname.replace(/\/$/, '') !== CHAT_PATH) return null;
  const params = new URLSearchParams(url.hash.replace(/^#/, ''));
  const server = params.get('server')?.trim();
  const channel = positiveId(params.get('channel'));
  const message = positiveId(params.get('message'));
  if (!server || channel === null || message === null) return null;
  return { server: normalizeServerUrl(server), channel, message };
}

/**
 * A link opened for a server other than the one on screen. The chat page
 * consumes it once that server's channel list has arrived; in-memory only,
 * like `pendingInvite`, because it is used within the same page load.
 */
export const pendingMessageLink = writable<MessageLink | null>(null);
