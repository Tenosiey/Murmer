import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, RoleInfo } from '../types';

function createRoleStore() {
  const { subscribe, update, set } = writable<Record<string, RoleInfo>>({});
  chat.on('role-update', (msg: Message) => {
    const user = msg.user;
    const role = (msg as any).role as string | undefined;
    const color = (msg as any).color as string | undefined;
    if (typeof user === 'string' && typeof role === 'string') {
      update(r => ({ ...r, [user]: { role, color } }));
    }
  });
  return { subscribe, set };
}

export const roles = createRoleStore();
