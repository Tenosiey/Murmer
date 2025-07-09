import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const STORAGE_KEY = 'murmer_volume';

let initial = 1;
if (browser) {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored !== null) {
    const num = parseFloat(stored);
    if (!isNaN(num)) initial = num;
  }
}

export const volume = writable(initial);

volume.subscribe((value) => {
  if (browser) {
    localStorage.setItem(STORAGE_KEY, String(value));
  }
});
