import { writable, get } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import { session } from './session';
import { roles } from './roles';
import type { Message } from '../types';

export interface ServerInfo {
  version: string;
}

/** Roles the server answers `get-server-info` requests for. */
const SERVER_INFO_ROLES = ['owner', 'admin'];

/**
 * Server details (currently the running server version), available only to
 * Owner and Admin users. The server enforces the role check; this store just
 * requests the info once the client learns its own role qualifies and holds
 * the response. Stays null for everyone else.
 */
function createServerInfoStore() {
  const { subscribe, set } = writable<ServerInfo | null>(null);
  let requested = false;

  function maybeRequest() {
    if (requested) return;
    if (get(connection) !== 'connected') return;
    const user = get(session).user;
    if (!user) return;
    const role = get(roles)[user]?.role?.toLowerCase();
    if (!role || !SERVER_INFO_ROLES.includes(role)) return;
    requested = true;
    chat.sendRaw({ type: 'get-server-info' });
  }

  chat.on('server-info', (msg: Message) => {
    const version = (msg as any).version;
    if (typeof version === 'string' && version.trim()) {
      set({ version: version.trim() });
    }
  });

  connection.subscribe((state) => {
    if (state === 'connected') {
      maybeRequest();
    } else {
      // New connection (or none) — forget the previous server's details.
      requested = false;
      set(null);
    }
  });

  // The own role usually arrives after the connection is established.
  roles.subscribe(maybeRequest);
  session.subscribe(maybeRequest);

  return { subscribe };
}

export const serverInfo = createServerInfoStore();
