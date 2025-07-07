import { writable } from 'svelte/store';
import { browser } from '$app/environment';

interface SettingsState {
  volume: number;
  inputDeviceId: string;
}

const VOLUME_KEY = 'murmer_volume';
const INPUT_KEY = 'murmer_input_device';

const initialState: SettingsState = {
  volume: browser ? parseFloat(localStorage.getItem(VOLUME_KEY) ?? '1') : 1,
  inputDeviceId: browser ? localStorage.getItem(INPUT_KEY) ?? '' : ''
};

export const settings = writable<SettingsState>(initialState);

settings.subscribe((s) => {
  if (!browser) return;
  localStorage.setItem(VOLUME_KEY, s.volume.toString());
  if (s.inputDeviceId) {
    localStorage.setItem(INPUT_KEY, s.inputDeviceId);
  } else {
    localStorage.removeItem(INPUT_KEY);
  }
});
