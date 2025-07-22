import { writable, get } from 'svelte/store';
import { servers } from './servers';
import { browser } from '$app/environment';

export interface StatusMap {
  [url: string]: boolean | null;
}

function toHttp(url: string): string {
  try {
    const u = new URL(url);
    u.protocol = u.protocol.replace('ws', 'http');
    if (u.pathname.endsWith('/ws')) u.pathname = u.pathname.slice(0, -3);
    return u.toString();
  } catch {
    return url;
  }
}

function createStatusStore() {
  const { subscribe, update, set } = writable<StatusMap>({});
  let interval: number | null = null;

  async function check(url: string): Promise<boolean> {
    try {
      const res = await fetch(toHttp(url), { method: 'HEAD' });
      return !!res;
    } catch {
      return false;
    }
  }

  async function checkAll() {
    const list = get(servers);
    for (const entry of list) {
      const ok = await check(entry.url);
      update((map) => ({ ...map, [entry.url]: ok }));
    }
  }

  function start() {
    if (!browser) return;
    stop();
    set({});
    checkAll();
    interval = window.setInterval(checkAll, 30000);
  }

  function stop() {
    if (interval !== null) {
      clearInterval(interval);
      interval = null;
    }
  }

  servers.subscribe(() => {
    if (browser) checkAll();
  });

  return { subscribe, start, stop };
}

export const serverStatus = createStatusStore();
