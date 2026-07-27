/**
 * The audio input/output devices offered in Settings.
 *
 * Enumerating once when the settings modal mounts was not enough on two
 * counts: before microphone permission has been granted the browser hands
 * back entries with empty labels (so the dropdowns showed opaque device id
 * hashes), and a headset plugged in afterwards never appeared at all. This
 * store re-enumerates on `devicechange` and whenever a capture starts, which
 * is the moment labels become available.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';
import { captureStream } from './voiceCapture';

export const audioInputs = writable<MediaDeviceInfo[]>([]);
export const audioOutputs = writable<MediaDeviceInfo[]>([]);

let refreshing: Promise<void> | null = null;

export function refreshAudioDevices(): Promise<void> {
  if (!browser || !navigator.mediaDevices?.enumerateDevices) return Promise.resolve();
  // Coalesce bursts: unplugging a USB headset fires several events at once.
  if (refreshing) return refreshing;
  refreshing = navigator.mediaDevices
    .enumerateDevices()
    .then((devices) => {
      audioInputs.set(devices.filter((d) => d.kind === 'audioinput'));
      audioOutputs.set(devices.filter((d) => d.kind === 'audiooutput'));
    })
    .catch((error) => {
      console.error('Failed to enumerate audio devices', error);
    })
    .finally(() => {
      refreshing = null;
    });
  return refreshing;
}

if (browser && navigator.mediaDevices) {
  navigator.mediaDevices.addEventListener?.('devicechange', () => {
    void refreshAudioDevices();
  });
  // A capture means permission was just granted, so the labels the previous
  // enumeration lacked are available now.
  captureStream.subscribe((stream) => {
    if (stream) void refreshAudioDevices();
  });
}
