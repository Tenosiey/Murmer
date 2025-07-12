import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from './chat';

function createOnlineStore() {
  const { subscribe, set } = writable<string[]>([]);
  chat.on('online-users', (msg: Message) => {
    if (Array.isArray(msg.users)) {
      set(msg.users as string[]);
    }
  });
  return { subscribe };
}

export const onlineUsers = createOnlineStore();
