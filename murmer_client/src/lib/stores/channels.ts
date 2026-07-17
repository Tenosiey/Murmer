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
          categoryId: typeof item.categoryId === 'number' ? item.categoryId : null,
          position: typeof item.position === 'number' ? item.position : 0
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
      const position = typeof raw.position === 'number' ? raw.position : 0;
      update((chs) =>
        chs.some((c) => c.id === id) ? chs : [...chs, { id, name, categoryId, position }]
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
      update((chs) =>
        chs.map((c) =>
          c.id === id
            ? { ...c, categoryId, position: typeof raw.position === 'number' ? raw.position : c.position }
            : c
        )
      );
    }
  });

  chat.on('channel-reorder', (msg: Message) => {
    const raw = msg as any;
    if (raw.voice === true) return;
    if (!Array.isArray(raw.order)) return;
    const categoryId = typeof raw.categoryId === 'number' ? raw.categoryId : null;
    const positions = new Map<number, number>(
      raw.order
        .filter((id: any): id is number => typeof id === 'number')
        .map((id: number, index: number) => [id, index])
    );
    update((chs) =>
      chs.map((c) =>
        positions.has(c.id) ? { ...c, categoryId, position: positions.get(c.id)! } : c
      )
    );
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

  /** Persist a new order for one category's channels; `order` lists every
      channel that shall live in `categoryId`, in display order. */
  function reorder(categoryId: number | null, order: number[], voice = false) {
    chat.sendRaw({ type: 'reorder-channels', categoryId, order, voice });
  }

  return { subscribe, set, create, remove, move, reorder };
}

export const channels = createChannelStore();
