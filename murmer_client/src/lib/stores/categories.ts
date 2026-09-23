import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, CategoryInfo } from '../types';

function createCategoryStore() {
  const { subscribe, set, update } = writable<CategoryInfo[]>([]);

  chat.on('category-list', (msg: Message) => {
    const list = msg.categories;
    if (Array.isArray(list)) {
      const items = list
        .filter(
          (c: any) =>
            c && typeof c === 'object' && typeof c.id === 'number' && typeof c.name === 'string'
        )
        .map((c: any) => ({
          id: c.id as number,
          name: c.name as string,
          position: typeof c.position === 'number' ? c.position : 0
        }));
      set(items);
    }
  });

  chat.on('category-add', (msg: Message) => {
    if (typeof msg.id === 'number' && typeof msg.name === 'string') {
      const info: CategoryInfo = {
        id: msg.id,
        name: msg.name,
        position: typeof msg.position === 'number' ? msg.position : 0
      };
      update((cats) => (cats.some((c) => c.id === info.id) ? cats : [...cats, info]));
    }
  });

  chat.on('category-update', (msg: Message) => {
    const { id, name } = msg;
    if (typeof id === 'number' && typeof name === 'string') {
      update((cats) => cats.map((c) => (c.id === id ? { ...c, name } : c)));
    }
  });

  chat.on('category-remove', (msg: Message) => {
    if (typeof msg.id === 'number') {
      update((cats) => cats.filter((c) => c.id !== msg.id));
    }
  });

  chat.on('category-reorder', (msg: Message) => {
    if (!Array.isArray(msg.order)) return;
    const positions = new Map<number, number>(
      msg.order
        .filter((id: any): id is number => typeof id === 'number')
        .map((id: number, index: number) => [id, index])
    );
    update((cats) =>
      cats.map((c) => (positions.has(c.id) ? { ...c, position: positions.get(c.id)! } : c))
    );
  });

  function create(name: string, position?: number) {
    const payload: Record<string, unknown> = { type: 'create-category', name };
    if (position !== undefined) payload.position = position;
    chat.sendRaw(payload);
  }

  function rename(id: number, name: string) {
    chat.sendRaw({ type: 'rename-category', id, name });
  }

  function remove(id: number) {
    chat.sendRaw({ type: 'delete-category', id });
  }

  /** Persist a new order for all categories; `order` lists every category id
      in display order. */
  function reorder(order: number[]) {
    chat.sendRaw({ type: 'reorder-categories', order });
  }

  return { subscribe, set, create, rename, remove, reorder };
}

export const categories = createCategoryStore();
