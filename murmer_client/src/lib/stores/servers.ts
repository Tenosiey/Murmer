import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export interface ServerEntry {
  url: string;
  name: string;
}

const STORAGE_KEY = 'murmer_servers';
const SELECTED_KEY = 'murmer_selected_server';

function loadServers(): ServerEntry[] {
  if (!browser) return [];
  const data = localStorage.getItem(STORAGE_KEY);
  try {
    if (!data) return [];
    const parsed = JSON.parse(data);
    if (Array.isArray(parsed)) {
      if (parsed.length && typeof parsed[0] === 'string') {
        return (parsed as string[]).map((url) => ({ url, name: url }));
      }
      return parsed as ServerEntry[];
    }
    return [];
  } catch {
    return [];
  }
}

function persist(list: ServerEntry[]) {
  if (browser) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list));
  }
}

const { subscribe, update } = writable<ServerEntry[]>(loadServers());

export const servers = {
  subscribe,
  add(entry: ServerEntry) {
    update((list) => {
      if (!list.some((s) => s.url === entry.url)) {
        const newList = [...list, entry];
        persist(newList);
        return newList;
      }
      return list;
    });
  },
  remove(url: string) {
    update((list) => {
      const newList = list.filter((s) => s.url !== url);
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
