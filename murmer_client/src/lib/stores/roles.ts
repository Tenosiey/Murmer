import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createRoleStore() {
  const { subscribe, update, set } = writable<Record<string, string>>({});
  chat.on('role-update', (msg: Message) => {
    const user = msg.user;
    const role = (msg as any).role;
    if (typeof user === 'string' && typeof role === 'string') {
      update(r => ({ ...r, [user]: role }));
    }
  });
  return { subscribe, set };
}

export const roles = createRoleStore();
