import { writable, derived } from 'svelte/store';
import { chat } from './chat';
import type { Message } from './chat';
import { onlineUsers } from './online';

function createAllUserStore() {
  const { subscribe, set } = writable<string[]>([]);
  chat.on('online-users', (msg: Message) => {
    if (Array.isArray((msg as any).all)) {
      set((msg as any).all as string[]);
    }
  });
  return { subscribe };
}

export const allUsers = createAllUserStore();
export const offlineUsers = derived([allUsers, onlineUsers], ([$all, $online]) =>
  $all.filter((u) => !$online.includes(u))
);
