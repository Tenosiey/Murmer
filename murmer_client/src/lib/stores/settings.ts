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

// Persist selected input and output devices
const IN_KEY = 'murmer_input_device';
const OUT_KEY = 'murmer_output_device';

let initialInput: string | null = null;
let initialOutput: string | null = null;

if (browser) {
  initialInput = localStorage.getItem(IN_KEY);
  initialOutput = localStorage.getItem(OUT_KEY);
}

export const inputDeviceId = writable<string | null>(initialInput);
export const outputDeviceId = writable<string | null>(initialOutput);

inputDeviceId.subscribe((value) => {
  if (!browser) return;
  if (value) localStorage.setItem(IN_KEY, value);
  else localStorage.removeItem(IN_KEY);
});

outputDeviceId.subscribe((value) => {
  if (!browser) return;
  if (value) localStorage.setItem(OUT_KEY, value);
  else localStorage.removeItem(OUT_KEY);
});
