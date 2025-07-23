import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const LEFT_KEY = 'murmer_left_width';
const RIGHT_KEY = 'murmer_right_width';

function load(key: string, def: number): number {
  if (!browser) return def;
  const raw = localStorage.getItem(key);
  const val = raw ? parseInt(raw) : NaN;
  return isNaN(val) ? def : val;
}

function persist(key: string, val: number) {
  if (browser) localStorage.setItem(key, String(val));
}

export const leftSidebarWidth = writable<number>(load(LEFT_KEY, 140));
export const rightSidebarWidth = writable<number>(load(RIGHT_KEY, 200));

leftSidebarWidth.subscribe((v) => persist(LEFT_KEY, v));
rightSidebarWidth.subscribe((v) => persist(RIGHT_KEY, v));

