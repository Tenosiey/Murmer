import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, ChannelInfo } from '../types';

function createChannelStore() {
  const { subscribe, set, update } = writable<ChannelInfo[]>([]);

  chat.on('channel-list', (msg: Message) => {
    const list = (msg as any).channels;
    if (Array.isArray(list)) {
      const items: ChannelInfo[] = list.map((item: any) => {
        if (typeof item === 'string') {
          return { name: item, categoryId: null };
        }
        return {
          name: typeof item.name === 'string' ? item.name : '',
          categoryId: typeof item.categoryId === 'number' ? item.categoryId : null
        };
      });
      set(items);
    }
  });

  chat.on('channel-add', (msg: Message) => {
    const raw = msg as any;
    const name = typeof raw.channel === 'string' ? raw.channel : null;
    if (name) {
      const categoryId = typeof raw.categoryId === 'number' ? raw.categoryId : null;
      update((chs) =>
        chs.some((c) => c.name === name) ? chs : [...chs, { name, categoryId }]
      );
    }
  });

  chat.on('channel-remove', (msg: Message) => {
    const name = (msg as any).channel;
    if (typeof name === 'string') {
      update((chs) => chs.filter((c) => c.name !== name));
    }
  });

  chat.on('channel-move', (msg: Message) => {
    const raw = msg as any;
    const name = typeof raw.channel === 'string' ? raw.channel : null;
    if (!name) return;
    const categoryId = typeof raw.categoryId === 'number' ? raw.categoryId : null;
    const isVoice = raw.voice === true;
    if (!isVoice) {
      update((chs) => chs.map((c) => (c.name === name ? { ...c, categoryId } : c)));
    }
  });

  function create(name: string, categoryId?: number | null) {
    const payload: Record<string, unknown> = { type: 'create-channel', name };
    if (categoryId != null) payload.categoryId = categoryId;
    chat.sendRaw(payload);
  }

  function remove(name: string) {
    chat.sendRaw({ type: 'delete-channel', name });
  }

  function move(channel: string, categoryId: number | null, voice = false) {
    chat.sendRaw({ type: 'move-channel', channel, categoryId, voice });
  }

  return { subscribe, set, create, remove, move };
}

export const channels = createChannelStore();
