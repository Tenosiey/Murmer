import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { chat } from './chat';
import { connection } from './connection';
import { selectedServer } from './servers';
import { dialogs } from './dialogs';
import type { Message } from '../types';

/**
 * Server identity (name, description, welcome message, icon) configured by
 * Admins/Owners in the server dashboard. The server sends a `server-identity`
 * frame after authentication and broadcasts it on every change; editing is
 * role-checked server-side. A trimmed copy is cached per server URL in
 * localStorage so the server selection page can show the identity of servers
 * the user is not currently connected to.
 */

export interface ServerIdentity {
  name: string;
  description: string;
  welcomeMessage: string;
  /** `/files/<key>` upload URL, or null when no icon is configured. */
  icon: string | null;
}

/** Cached subset shown on the server selection page. */
export interface CachedServerIdentity {
  name: string;
  description: string;
  icon: string | null;
}

const CACHE_KEY = 'murmer_server_identities';

function loadCache(): Record<string, CachedServerIdentity> {
  if (!browser) return {};
  try {
    const parsed = JSON.parse(localStorage.getItem(CACHE_KEY) ?? '{}');
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function parseIdentity(msg: Message): ServerIdentity {
  const payload = msg as any;
  return {
    name: typeof payload.name === 'string' ? payload.name : '',
    description: typeof payload.description === 'string' ? payload.description : '',
    welcomeMessage: typeof payload.welcomeMessage === 'string' ? payload.welcomeMessage : '',
    // Only accept upload paths; anything else cannot be a valid icon and
    // must not end up concatenated onto the server's HTTP base URL.
    icon:
      typeof payload.icon === 'string' && payload.icon.startsWith('/files/') ? payload.icon : null
  };
}

function createServerIdentityStores() {
  /** null until the connected server announced its identity. */
  const identity = writable<ServerIdentity | null>(null);
  const cache = writable<Record<string, CachedServerIdentity>>(loadCache());

  chat.on('server-identity', (msg: Message) => {
    const parsed = parseIdentity(msg);
    identity.set(parsed);
    const url = get(selectedServer);
    if (!url) return;
    cache.update((map) => {
      const next = {
        ...map,
        [url]: { name: parsed.name, description: parsed.description, icon: parsed.icon }
      };
      if (browser) {
        localStorage.setItem(CACHE_KEY, JSON.stringify(next));
      }
      return next;
    });
  });

  // The welcome message is delivered once, to members connecting to this
  // server for the very first time.
  chat.on('welcome', (msg: Message) => {
    const payload = msg as any;
    if (typeof payload.message !== 'string' || !payload.message.trim()) return;
    const serverName = typeof payload.serverName === 'string' ? payload.serverName.trim() : '';
    dialogs.alert({
      title: serverName ? `Welcome to ${serverName}` : 'Welcome',
      message: payload.message
    });
  });

  connection.subscribe((state) => {
    if (state !== 'connected') {
      // New connection (or none) — forget the previous server's identity.
      identity.set(null);
    }
  });

  /**
   * Update any subset of the identity (Admin/Owner only; enforced
   * server-side). `icon: null` removes the icon; the confirmation arrives as
   * a broadcast `server-identity` frame which updates this store.
   */
  function save(fields: Partial<ServerIdentity>): void {
    const frame: Record<string, unknown> = { type: 'set-server-identity' };
    if (fields.name !== undefined) frame.name = fields.name;
    if (fields.description !== undefined) frame.description = fields.description;
    if (fields.welcomeMessage !== undefined) frame.welcomeMessage = fields.welcomeMessage;
    if (fields.icon !== undefined) frame.icon = fields.icon;
    chat.sendRaw(frame);
  }

  return { identity, cache, save };
}

const stores = createServerIdentityStores();

export const serverIdentity = {
  subscribe: stores.identity.subscribe,
  save: stores.save
};
export const serverIdentityCache = { subscribe: stores.cache.subscribe };
