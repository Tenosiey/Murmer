/**
 * Camera video in voice channels.
 *
 * Owns the local capture and the bookkeeping of whose camera is on. The video
 * itself is *not* a mesh of its own: it travels over the voice peer
 * connections through `voice.setCameraTrack`, on a transceiver those
 * connections negotiate whether or not a camera is ever switched on. See
 * `voice/manager.ts`.
 *
 * The frames here (`webcam-start`, `webcam-stop`, `webcam-active`) therefore
 * carry no SDP — they are only the announcement that says whose tile to show.
 * A track is delivered by the transceiver either way, so without them a camera
 * that was switched off would still look live.
 */
import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { chat } from './chat';
import { connection } from './connection';
import { voice } from './voice';
import { refreshVideoDevices } from './videoDevices';
import {
  cameraPreset,
  isCameraQuality,
  openCamera,
  DEFAULT_CAMERA_QUALITY,
  type CameraQuality
} from '../voice/camera';
import type { Message } from '../types';

const DEVICE_KEY = 'murmer_camera_device';
const QUALITY_KEY = 'murmer_camera_quality';
const MIRROR_KEY = 'murmer_camera_mirror';

/**
 * The camera to open. Persisted like the microphone device: which of the two
 * webcams on this machine is the good one does not change between calls.
 */
export const cameraDeviceId = writable<string | null>(
  browser ? localStorage.getItem(DEVICE_KEY) : null
);

/** Resolution/bitrate preset, validated on load — localStorage is editable. */
export const cameraQuality = writable<CameraQuality>(
  browser && isCameraQuality(localStorage.getItem(QUALITY_KEY))
    ? (localStorage.getItem(QUALITY_KEY) as CameraQuality)
    : DEFAULT_CAMERA_QUALITY
);

/**
 * Whether our own preview is flipped. On by default because an unmirrored
 * self-view is the one thing every video app gets asked about: people expect
 * the mirror they see in the morning. Only the *preview* is flipped — the
 * peers receive the picture as the camera saw it.
 */
export const mirrorSelfView = writable<boolean>(
  browser ? localStorage.getItem(MIRROR_KEY) !== 'false' : true
);

cameraDeviceId.subscribe((value) => {
  if (!browser) return;
  if (value) localStorage.setItem(DEVICE_KEY, value);
  else localStorage.removeItem(DEVICE_KEY);
});

cameraQuality.subscribe((value) => {
  if (browser) localStorage.setItem(QUALITY_KEY, value);
});

mirrorSelfView.subscribe((value) => {
  if (browser) localStorage.setItem(MIRROR_KEY, String(value));
});

/** Our own capture while the camera is on, for the self-preview tile. */
export const localCameraStream = writable<MediaStream | null>(null);

/** Whether our camera is on. Mirrors `localCameraStream` having a stream. */
export const cameraOn = writable<boolean>(false);

/**
 * Whose camera is on, per voice channel: channel id -> account names. Fed by
 * the server's announcements, including the `webcam-active` catch-up a client
 * gets when it joins a channel where cameras are already running.
 */
export const activeWebcams = writable<Record<number, string[]>>({});

/**
 * The running capture: who and where it was announced for (needed for the
 * `webcam-stop`) and which settings it was opened with, so a settings change
 * that changes nothing does not re-open the device.
 */
let session: {
  user: string;
  channelId: number;
  deviceId: string | null;
  quality: CameraQuality;
} | null = null;

/** Serialises starts and stops so a fast toggle cannot leave two captures. */
let pending: Promise<void> = Promise.resolve();

function addActive(channelId: number, user: string): void {
  activeWebcams.update((all) => {
    const current = all[channelId] ?? [];
    if (current.includes(user)) return all;
    return { ...all, [channelId]: [...current, user] };
  });
}

function removeActive(channelId: number, user: string): void {
  activeWebcams.update((all) => {
    const current = all[channelId];
    if (!current) return all;
    return { ...all, [channelId]: current.filter((name) => name !== user) };
  });
}

/** Drop the capture and detach it from every peer. */
function releaseCapture(): void {
  const stream = get(localCameraStream);
  if (stream) {
    for (const track of stream.getTracks()) track.stop();
  }
  localCameraStream.set(null);
  cameraOn.set(false);
  voice.setCameraTrack(null);
}

/**
 * Switch the camera on and tell the channel.
 *
 * Rejects without changing any state when the camera cannot be opened, so a
 * denied permission prompt leaves the button off rather than stuck on.
 */
export function startCamera(user: string, channelId: number): Promise<void> {
  pending = pending
    .then(async () => {
      if (get(localCameraStream)) return;
      const quality = get(cameraQuality);
      const deviceId = get(cameraDeviceId);
      const stream = await openCamera(quality, deviceId);
      const track = stream.getVideoTracks()[0];
      if (!track) {
        for (const t of stream.getTracks()) t.stop();
        throw new Error('The camera returned no video track');
      }
      // A face is motion, not detail: the encoder should soften before it
      // stutters. `maintain-framerate` on the sender says the same thing.
      track.contentHint = 'motion';
      // Unplugging the camera ends the track and leaves a frozen tile up.
      track.addEventListener('ended', () => {
        if (get(localCameraStream) === stream) void stopCamera();
      });

      session = { user, channelId, deviceId, quality };
      localCameraStream.set(stream);
      cameraOn.set(true);
      voice.setCameraTrack(track, cameraPreset(quality).maxBitrate);
      addActive(channelId, user);
      chat.sendRaw({ type: 'webcam-start', user, channelId });
      // Permission was just granted, so the labels the last enumeration
      // lacked are available now.
      void refreshVideoDevices();
    })
    .catch((error) => {
      releaseCapture();
      session = null;
      throw error;
    });
  return pending;
}

/** Switch the camera off and tell the channel. Safe to call when it is off. */
export function stopCamera(): Promise<void> {
  pending = pending.then(() => {
    const active = session;
    releaseCapture();
    session = null;
    if (!active) return;
    removeActive(active.channelId, active.user);
    chat.sendRaw({ type: 'webcam-stop', user: active.user, channelId: active.channelId });
  });
  return pending;
}

/** Toggle the camera, resolving to whether it is on afterwards. */
export async function toggleCamera(user: string, channelId: number): Promise<boolean> {
  if (get(localCameraStream)) {
    await stopCamera();
    return false;
  }
  await startCamera(user, channelId);
  return true;
}

/**
 * Apply the current device and quality to the running capture, without
 * dropping out of the channel's camera list.
 *
 * The settings subscriptions call this on every change; it is a no-op unless
 * the camera is on *and* the settings actually differ from the ones it was
 * opened with, so re-opening the device stays tied to a real change.
 */
export function restartCamera(): Promise<void> {
  pending = pending
    .then(async () => {
      const active = session;
      const previous = get(localCameraStream);
      if (!active || !previous) return;
      const quality = get(cameraQuality);
      const deviceId = get(cameraDeviceId);
      if (active.deviceId === deviceId && active.quality === quality) return;

      const stream = await openCamera(quality, deviceId);
      const track = stream.getVideoTracks()[0];
      // The session may have ended while the device was opening.
      if (!track || session !== active || get(localCameraStream) !== previous) {
        for (const t of stream.getTracks()) t.stop();
        return;
      }
      track.contentHint = 'motion';
      track.addEventListener('ended', () => {
        if (get(localCameraStream) === stream) void stopCamera();
      });
      for (const t of previous.getTracks()) t.stop();
      active.deviceId = deviceId;
      active.quality = quality;
      localCameraStream.set(stream);
      // `replaceTrack`, so the peers see a new picture without renegotiating
      // and nobody's tile goes black.
      voice.setCameraTrack(track, cameraPreset(quality).maxBitrate);
    })
    .catch((error) => {
      console.error('Failed to switch the camera:', error);
    });
  return pending;
}

// Both settings apply to the running capture, not just to the next one: a
// dropdown that visibly does nothing until you rejoin reads as broken.
// `restartCamera` returns immediately while the camera is off, which is what
// makes the initial (synchronous) subscribe call a no-op.
cameraDeviceId.subscribe(() => void restartCamera());
cameraQuality.subscribe(() => void restartCamera());

chat.on('webcam-start', (msg: Message) => {
  const { user, channelId } = msg;
  if (typeof user === 'string' && typeof channelId === 'number') {
    addActive(channelId, user);
  }
});

chat.on('webcam-stop', (msg: Message) => {
  const { user, channelId } = msg;
  if (typeof user === 'string' && typeof channelId === 'number') {
    removeActive(channelId, user);
    // The server ends cameras on voice-leave and on disconnect too, so this is
    // also where our own capture stops when something else decided it had.
    if (session && session.user === user && session.channelId === channelId) {
      void stopCamera();
    }
  }
});

chat.on('webcam-active', (msg: Message) => {
  const { channelId, users } = msg;
  if (typeof channelId !== 'number' || !Array.isArray(users)) return;
  const names = users.filter((name): name is string => typeof name === 'string');
  activeWebcams.update((all) => {
    const merged = [...new Set([...(all[channelId] ?? []), ...names])];
    return { ...all, [channelId]: merged };
  });
});

connection.subscribe((state) => {
  if (state === 'connected') return;
  // Without a connection there is no signaling to announce a camera with, and
  // channel ids are only unique per server — a stale list would put phantom
  // camera badges on the next server's members.
  void stopCamera();
  activeWebcams.set({});
});

if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => {
    releaseCapture();
  });
}
