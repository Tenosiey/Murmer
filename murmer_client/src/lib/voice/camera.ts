/**
 * Opening the camera for video in a voice channel.
 *
 * The capture side of the webcam feature, kept apart from the store that owns
 * its lifecycle so the constraint building — the part with the fallbacks that
 * only fire on hardware nobody has to hand — can be tested as a plain
 * function. The video itself is sent over the voice peer connections; see
 * `manager.ts`.
 */

/** One entry of `CAMERA_PRESETS`: what to ask the camera and the encoder for. */
export interface CameraPreset {
  width: number;
  height: number;
  /** Encoder cap in bits per second. */
  maxBitrate: number;
}

/**
 * The resolutions offered in the camera controls.
 *
 * Deliberately smaller and cheaper than the screen-share presets. Video is a
 * full mesh like the audio: every camera is encoded once per peer and sent to
 * each of them separately, so a channel of five with cameras on costs each
 * machine four encodes and four decodes. 720p is the ceiling for that reason,
 * not because the encoder could not do more.
 */
export const CAMERA_PRESETS = {
  '360p': { width: 640, height: 360, maxBitrate: 500_000 },
  '480p': { width: 854, height: 480, maxBitrate: 800_000 },
  '720p': { width: 1280, height: 720, maxBitrate: 1_500_000 }
} as const satisfies Record<string, CameraPreset>;

export type CameraQuality = keyof typeof CAMERA_PRESETS;

export const CAMERA_QUALITIES = Object.keys(CAMERA_PRESETS) as CameraQuality[];

/** The default quality: legible at tile size and cheap enough for a full mesh. */
export const DEFAULT_CAMERA_QUALITY: CameraQuality = '480p';

/** Frames per second requested from every camera. */
export const CAMERA_FRAME_RATE = 30;

/** Whether `value` names one of the presets (used to vet persisted state). */
export function isCameraQuality(value: unknown): value is CameraQuality {
  return typeof value === 'string' && value in CAMERA_PRESETS;
}

/** The preset for a quality, falling back to the default for anything else. */
export function cameraPreset(quality: unknown): CameraPreset {
  return CAMERA_PRESETS[isCameraQuality(quality) ? quality : DEFAULT_CAMERA_QUALITY];
}

/**
 * The `getUserMedia` video constraints for a preset and camera.
 *
 * Resolution is `ideal` rather than `exact` on purpose: a camera that cannot
 * do 720p should hand back the closest thing it has, not refuse to open. The
 * device id is the only `exact` part, because "some other camera" is never
 * what the user picked — `openCamera` handles that rejection instead.
 */
export function cameraConstraints(
  quality: unknown,
  deviceId?: string | null
): MediaTrackConstraints {
  const preset = cameraPreset(quality);
  const constraints: MediaTrackConstraints = {
    width: { ideal: preset.width },
    height: { ideal: preset.height },
    frameRate: { ideal: CAMERA_FRAME_RATE }
  };
  if (deviceId) constraints.deviceId = { exact: deviceId };
  return constraints;
}

/**
 * Whether a `getUserMedia` rejection means "that device is gone" rather than
 * "you may not record". Only the former is worth retrying on the default
 * camera — retrying a denied permission would just prompt again. Same rule as
 * `capture.ts` applies to the microphone.
 */
function isDeviceUnavailable(error: unknown): boolean {
  const name = (error as { name?: string } | null)?.name;
  return name === 'OverconstrainedError' || name === 'NotFoundError' || name === 'NotReadableError';
}

/**
 * Open the camera, falling back to the default device when the configured one
 * is no longer plugged in. Audio is never requested here: the microphone is
 * already open through `capture.ts`, and a second capture of it would be a
 * second echo-cancellation domain.
 */
export async function openCamera(
  quality: unknown,
  deviceId?: string | null
): Promise<MediaStream> {
  if (deviceId) {
    try {
      return await navigator.mediaDevices.getUserMedia({
        video: cameraConstraints(quality, deviceId)
      });
    } catch (error) {
      if (!isDeviceUnavailable(error)) throw error;
      console.warn('Configured camera unavailable, using the default device:', error);
    }
  }
  return navigator.mediaDevices.getUserMedia({ video: cameraConstraints(quality) });
}
