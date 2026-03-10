import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, ChannelInfo } from '../types';

function createChannelStore() {
  const { subscribe, set, update } = writable<ChannelInfo[]>([]);

  chat.on('channel-list', (msg: Message) => {
    const list = (msg as any).channels;
    if (Array.isArray(list)) {
      const items: ChannelInfo[] = list
        .filter((item: any) => typeof item === 'object' && item !== null && typeof item.id === 'number')
        .map((item: any) => ({
          id: item.id as number,
          name: typeof item.name === 'string' ? item.name : '',
          categoryId: typeof item.categoryId === 'number' ? item.categoryId : null
        }));
      set(items);
    }
  });

  chat.on('channel-add', (msg: Message) => {
    const raw = msg as any;
    const id = typeof raw.channelId === 'number' ? raw.channelId : null;
    const name = typeof raw.name === 'string' ? raw.name : null;
    if (id !== null && name) {
      const categoryId = typeof raw.categoryId === 'number' ? raw.categoryId : null;
      update((chs) =>
        chs.some((c) => c.id === id) ? chs : [...chs, { id, name, categoryId }]
      );
    }
  });

  chat.on('channel-remove', (msg: Message) => {
    const id = (msg as any).channelId;
    if (typeof id === 'number') {
      update((chs) => chs.filter((c) => c.id !== id));
    }
  });

  chat.on('channel-move', (msg: Message) => {
    const raw = msg as any;
    const id = typeof raw.channelId === 'number' ? raw.channelId : null;
    if (id === null) return;
    const categoryId = typeof raw.categoryId === 'number' ? raw.categoryId : null;
    const isVoice = raw.voice === true;
    if (!isVoice) {
      update((chs) => chs.map((c) => (c.id === id ? { ...c, categoryId } : c)));
    }
  });

  function create(name: string, categoryId?: number | null) {
    const payload: Record<string, unknown> = { type: 'create-channel', name };
    if (categoryId != null) payload.categoryId = categoryId;
    chat.sendRaw(payload);
  }

  function remove(channelId: number) {
    chat.sendRaw({ type: 'delete-channel', channelId });
  }

  function move(channelId: number, categoryId: number | null, voice = false) {
    chat.sendRaw({ type: 'move-channel', channelId, categoryId, voice });
  }

  return { subscribe, set, create, remove, move };
}

export const channels = createChannelStore();
