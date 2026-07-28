/**
 * Opening the microphone the way a voice session does.
 *
 * Shared by the voice manager, the settings level meter and the microphone
 * test: all three have to capture the same signal, because a meter or a test
 * recording made with different constraints would not show what the other
 * members actually hear.
 */
import { get } from 'svelte/store';
import {
  inputDeviceId,
  echoCancellation,
  noiseSuppressionMode,
  autoGainControl
} from '../stores/settings';

/**
 * The microphone processing settings, as `getUserMedia` constraints.
 *
 * The platform's own suppressor is asked for only in `browser` mode: in
 * `rnnoise` mode it would filter the capture before RNNoise ever sees it, which
 * gives the better of the two filters worse input to work from.
 */
export function micProcessingConstraints(): MediaTrackConstraints {
  return {
    echoCancellation: get(echoCancellation),
    noiseSuppression: get(noiseSuppressionMode) === 'browser',
    autoGainControl: get(autoGainControl)
  };
}

/**
 * Whether a `getUserMedia` rejection means "that device is gone" rather than
 * "you may not record". Only the former is worth retrying on the default
 * device — retrying a denied permission would just prompt again.
 */
function isDeviceUnavailable(error: unknown): boolean {
  const name = (error as { name?: string } | null)?.name;
  return name === 'OverconstrainedError' || name === 'NotFoundError' || name === 'NotReadableError';
}

/**
 * Open the microphone with the current device and processing settings.
 *
 * A device that has since been unplugged rejects with `exact`, which used to
 * fail the whole join; fall back to the default device rather than leaving the
 * user unable to talk because of a stale setting.
 */
export async function openMicrophone(): Promise<MediaStream> {
  const audio = micProcessingConstraints();
  const device = get(inputDeviceId);
  if (device) {
    try {
      return await navigator.mediaDevices.getUserMedia({
        audio: { ...audio, deviceId: { exact: device } }
      });
    } catch (error) {
      if (!isDeviceUnavailable(error)) throw error;
      console.warn('Configured microphone unavailable, using the default device:', error);
    }
  }
  return navigator.mediaDevices.getUserMedia({ audio });
}
