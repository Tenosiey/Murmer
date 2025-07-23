import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createChannelStore() {
  const { subscribe, set, update } = writable<string[]>([]);

  chat.on('channel-list', (msg: Message) => {
    const list = (msg as any).channels;
    if (Array.isArray(list)) {
      set(list as string[]);
    }
  });

  chat.on('channel-add', (msg: Message) => {
    const name = (msg as any).channel;
    if (typeof name === 'string') {
      update((chs) => (chs.includes(name) ? chs : [...chs, name]));
    }
  });

  chat.on('channel-remove', (msg: Message) => {
    const name = (msg as any).channel;
    if (typeof name === 'string') {
      update((chs) => chs.filter((c) => c !== name));
    }
  });

  function create(name: string) {
    chat.sendRaw({ type: 'create-channel', name });
  }

  function remove(name: string) {
    chat.sendRaw({ type: 'delete-channel', name });
  }

  return { subscribe, set, create, remove };
}

export const channels = createChannelStore();
