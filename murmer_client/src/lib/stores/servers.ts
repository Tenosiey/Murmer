import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';

export interface ServerEntry {
  url: string;
  name: string;
  password?: string;
  /**
   * A server-issued invite code, kept alongside the password because it is the
   * credential the first connection presents. The server records the redeeming
   * key as a member, so later connections are admitted on that membership and
   * a stale code here costs nothing — it is sent, ignored, and never spends
   * another of the invite's uses.
   */
  invite?: string;
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

const internal = writable<ServerEntry[]>(loadServers());
const { subscribe, update } = internal;

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
  /**
   * Add an entry, or replace the one already saved for that URL in place.
   *
   * Used by the invite flow, where the point is to refresh the name and
   * password of a server the user may already have. `add` deliberately keeps
   * an existing entry untouched instead.
   */
  upsert(entry: ServerEntry) {
    update((list) => {
      const index = list.findIndex((s) => s.url === entry.url);
      const newList = index === -1 ? [...list, entry] : list.with(index, entry);
      persist(newList);
      return newList;
    });
  },
  remove(url: string) {
    update((list) => {
      const newList = list.filter((s) => s.url !== url);
      persist(newList);
      return newList;
    });
  },
  get(url: string): ServerEntry | undefined {
    return get(internal).find((s) => s.url === url);
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
