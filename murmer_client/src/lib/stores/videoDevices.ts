/**
 * The cameras offered in the video controls.
 *
 * Separate from `audioDevices.ts` because it re-enumerates on a different
 * signal: camera labels stay empty until *camera* permission is granted, which
 * happens the first time somebody switches their camera on and not when the
 * microphone opens. Same failure otherwise — a dropdown full of device id
 * hashes, and a webcam plugged in later never appearing at all.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export const videoInputs = writable<MediaDeviceInfo[]>([]);

let refreshing: Promise<void> | null = null;

export function refreshVideoDevices(): Promise<void> {
  if (!browser || !navigator.mediaDevices?.enumerateDevices) return Promise.resolve();
  // Coalesce bursts: plugging in a USB camera fires several events at once.
  if (refreshing) return refreshing;
  refreshing = navigator.mediaDevices
    .enumerateDevices()
    .then((devices) => {
      videoInputs.set(devices.filter((d) => d.kind === 'videoinput'));
    })
    .catch((error) => {
      console.error('Failed to enumerate cameras', error);
    })
    .finally(() => {
      refreshing = null;
    });
  return refreshing;
}

if (browser && navigator.mediaDevices) {
  navigator.mediaDevices.addEventListener?.('devicechange', () => {
    void refreshVideoDevices();
  });
}
