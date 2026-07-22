import { writable, get } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import { session } from './session';
import { myPermissions } from './permissions';
import { hasPermission, PERMISSIONS } from '../chat/permissions';
import type { Message } from '../types';

export interface ServerInfo {
  version: string;
}

/**
 * Server details (currently the running server version), available only to
 * users whose roles grant VIEW_SERVER_INFO. The server enforces the check;
 * this store just requests the info once the client learns it qualifies and
 * holds the response. Stays null for everyone else.
 */
function createServerInfoStore() {
  const { subscribe, set } = writable<ServerInfo | null>(null);
  let requested = false;

  function maybeRequest() {
    if (requested) return;
    if (get(connection) !== 'connected') return;
    if (!get(session).user) return;
    if (!hasPermission(get(myPermissions), PERMISSIONS.VIEW_SERVER_INFO)) return;
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

  // Permissions usually resolve after the connection is established.
  myPermissions.subscribe(maybeRequest);
  session.subscribe(maybeRequest);

  return { subscribe };
}

export const serverInfo = createServerInfoStore();
