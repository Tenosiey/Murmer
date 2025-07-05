import { writable } from 'svelte/store';

/** Simple session store holding the logged in user */
export const session = writable<{ user: string | null }>({ user: null });
