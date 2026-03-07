import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, CategoryInfo } from '../types';

function createCategoryStore() {
  const { subscribe, set, update } = writable<CategoryInfo[]>([]);

  chat.on('category-list', (msg: Message) => {
    const list = (msg as any).categories;
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
    const raw = msg as any;
    if (typeof raw.id === 'number' && typeof raw.name === 'string') {
      const info: CategoryInfo = {
        id: raw.id,
        name: raw.name,
        position: typeof raw.position === 'number' ? raw.position : 0
      };
      update((cats) => (cats.some((c) => c.id === info.id) ? cats : [...cats, info]));
    }
  });

  chat.on('category-update', (msg: Message) => {
    const raw = msg as any;
    if (typeof raw.id === 'number' && typeof raw.name === 'string') {
      update((cats) => cats.map((c) => (c.id === raw.id ? { ...c, name: raw.name } : c)));
    }
  });

  chat.on('category-remove', (msg: Message) => {
    const raw = msg as any;
    if (typeof raw.id === 'number') {
      update((cats) => cats.filter((c) => c.id !== raw.id));
    }
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

  return { subscribe, set, create, rename, remove };
}

export const categories = createCategoryStore();
