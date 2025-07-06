import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const STORAGE_KEY = 'murmer_user';

const initial = browser ? localStorage.getItem(STORAGE_KEY) : null;

/** Simple session store holding the logged in user */
export const session = writable<{ user: string | null }>({ user: initial });

session.subscribe((value) => {
  if (!browser) return;
  if (value.user) {
    localStorage.setItem(STORAGE_KEY, value.user);
  } else {
    localStorage.removeItem(STORAGE_KEY);
  }
});
