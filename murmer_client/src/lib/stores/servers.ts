import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const STORAGE_KEY = 'murmer_servers';
const SELECTED_KEY = 'murmer_selected_server';

function loadServers(): string[] {
  if (!browser) return [];
  const data = localStorage.getItem(STORAGE_KEY);
  try {
    return data ? JSON.parse(data) : [];
  } catch {
    return [];
  }
}

function persist(list: string[]) {
  if (browser) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list));
  }
}

const { subscribe, update } = writable<string[]>(loadServers());

export const servers = {
  subscribe,
  add(server: string) {
    update((list) => {
      if (!list.includes(server)) {
        const newList = [...list, server];
        persist(newList);
        return newList;
      }
      return list;
    });
  },
  remove(server: string) {
    update((list) => {
      const newList = list.filter((s) => s !== server);
      persist(newList);
      return newList;
    });
  }
};

const initialSelected = browser ? localStorage.getItem(SELECTED_KEY) : null;
export const selectedServer = writable<string | null>(initialSelected);

selectedServer.subscribe((value) => {
  if (browser) {
    if (value) {
      localStorage.setItem(SELECTED_KEY, value);
    } else {
      localStorage.removeItem(SELECTED_KEY);
    }
  }
});
