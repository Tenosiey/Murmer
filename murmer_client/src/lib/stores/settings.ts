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

// The app's own blips (join, leave, mute, unmute). Kept separate from the
// voice volume, which used to drive both: these are short and near-constant in
// loudness, so the level that makes them a discreet cue is nothing like the
// level that makes a quiet talker audible, and turning people up should not
// turn the notifications up with them.
const APP_SOUND_VOLUME_KEY = 'murmer_app_sound_volume';

let initialAppSoundVolume = 1;
if (browser) {
  const stored = localStorage.getItem(APP_SOUND_VOLUME_KEY);
  if (stored !== null) {
    const num = parseFloat(stored);
    // A media element's volume is a 0-1 fraction; anything else is rejected
    // rather than clamped, since it can only come from a hand-edited entry.
    if (!isNaN(num) && num >= 0 && num <= 1) initialAppSoundVolume = num;
  }
}

export const appSoundVolume = writable<number>(initialAppSoundVolume);

appSoundVolume.subscribe((value) => {
  if (browser) localStorage.setItem(APP_SOUND_VOLUME_KEY, String(value));
});

// Screen share audio, heard by the viewer. Kept separate from the voice
// volume: a game or video is usually far louder than the people talking over
// it, and turning one down must not turn the other down with it.
const SCREENSHARE_VOLUME_KEY = 'murmer_screenshare_volume';
const SCREENSHARE_MUTE_KEY = 'murmer_screenshare_muted';

let initialScreenShareVolume = 1;
if (browser) {
  const stored = localStorage.getItem(SCREENSHARE_VOLUME_KEY);
  if (stored !== null) {
    const num = parseFloat(stored);
    // A media element's volume is a 0-1 fraction; anything else is rejected
    // rather than clamped, since it can only come from a hand-edited entry.
    if (!isNaN(num) && num >= 0 && num <= 1) initialScreenShareVolume = num;
  }
}

export const screenShareVolume = writable<number>(initialScreenShareVolume);
export const screenShareMuted = writable<boolean>(
  browser ? localStorage.getItem(SCREENSHARE_MUTE_KEY) === 'true' : false
);

screenShareVolume.subscribe((value) => {
  if (browser) localStorage.setItem(SCREENSHARE_VOLUME_KEY, String(value));
});

screenShareMuted.subscribe((value) => {
  if (browser) localStorage.setItem(SCREENSHARE_MUTE_KEY, String(value));
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

// Mute states
const MIC_MUTE_KEY = 'murmer_mic_muted';
const OUT_MUTE_KEY = 'murmer_output_muted';

let initialMicMuted = false;
let initialOutputMuted = false;

if (browser) {
  initialMicMuted = localStorage.getItem(MIC_MUTE_KEY) === 'true';
  initialOutputMuted = localStorage.getItem(OUT_MUTE_KEY) === 'true';
}

export const microphoneMuted = writable<boolean>(initialMicMuted);
export const outputMuted = writable<boolean>(initialOutputMuted);

microphoneMuted.subscribe((value) => {
  if (browser) {
    localStorage.setItem(MIC_MUTE_KEY, String(value));
  }
});

outputMuted.subscribe((value) => {
  if (browser) {
    localStorage.setItem(OUT_MUTE_KEY, String(value));
  }
});

// Individual user volumes
const USER_VOLUMES_KEY = 'murmer_user_volumes';

/** Upper bound for the per-user volume. Values above 1 are a real boost: the
    remote stream is amplified through a gain node, since an `<audio>` element's
    own `volume` cannot exceed 1. Capped at 2 (200%) because more than that
    turns clipping and background noise up just as much as the voice. */
export const MAX_USER_VOLUME = 2;

/** localStorage is user-writable, so every stored entry is re-validated
    instead of trusted — a bogus value would otherwise end up as a gain. */
function sanitizeUserVolumes(raw: unknown): Record<string, number> {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {};
  const result: Record<string, number> = {};
  for (const [user, value] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof value !== 'number' || !isFinite(value)) continue;
    result[user] = Math.max(0, Math.min(MAX_USER_VOLUME, value));
  }
  return result;
}

let initialUserVolumes: Record<string, number> = {};
if (browser) {
  const stored = localStorage.getItem(USER_VOLUMES_KEY);
  if (stored) {
    try {
      initialUserVolumes = sanitizeUserVolumes(JSON.parse(stored));
    } catch (e) {
      console.error('Failed to parse user volumes from localStorage', e);
    }
  }
}

export const userVolumes = writable<Record<string, number>>(initialUserVolumes);

userVolumes.subscribe((value) => {
  if (browser) {
    localStorage.setItem(USER_VOLUMES_KEY, JSON.stringify(value));
  }
});

export function setUserVolume(userId: string, volume: number) {
  userVolumes.update(volumes => ({
    ...volumes,
    [userId]: Math.max(0, Math.min(MAX_USER_VOLUME, volume))
  }));
}

export function getUserVolume(userId: string): number {
  let currentVolumes: Record<string, number> = {};
  userVolumes.subscribe(volumes => currentVolumes = volumes)();
  return currentVolumes[userId] ?? 1.0;
}

// Microphone processing (applied as getUserMedia constraints)
const ECHO_CANCEL_KEY = 'murmer_echo_cancellation';
const NOISE_SUPPRESS_KEY = 'murmer_noise_suppression';
const AUTO_GAIN_KEY = 'murmer_auto_gain';

function loadBool(key: string, def: boolean): boolean {
  if (!browser) return def;
  const stored = localStorage.getItem(key);
  return stored === null ? def : stored === 'true';
}

export const echoCancellation = writable<boolean>(loadBool(ECHO_CANCEL_KEY, true));
export const noiseSuppression = writable<boolean>(loadBool(NOISE_SUPPRESS_KEY, true));
export const autoGainControl = writable<boolean>(loadBool(AUTO_GAIN_KEY, true));

echoCancellation.subscribe((value) => {
  if (browser) localStorage.setItem(ECHO_CANCEL_KEY, String(value));
});

noiseSuppression.subscribe((value) => {
  if (browser) localStorage.setItem(NOISE_SUPPRESS_KEY, String(value));
});

autoGainControl.subscribe((value) => {
  if (browser) localStorage.setItem(AUTO_GAIN_KEY, String(value));
});

// Input (microphone) gain, applied as a plain multiplication on the captured
// signal before voice detection and before the transmission gate.
const MIC_GAIN_KEY = 'murmer_mic_gain';

/** Upper bound for the input gain. A quiet headset microphone needs real
    amplification to be audible, but this is a straight multiply — past 3x
    (300%) the room noise is louder than most people's voice was to begin
    with, and the encoder starts clipping on peaks. */
export const MAX_MIC_GAIN = 3;

export function clampMicGain(value: number): number {
  if (!isFinite(value)) return 1;
  return Math.max(0, Math.min(MAX_MIC_GAIN, value));
}

let initialMicGain = 1;
if (browser) {
  const stored = localStorage.getItem(MIC_GAIN_KEY);
  if (stored !== null) {
    const num = parseFloat(stored);
    if (!isNaN(num)) initialMicGain = clampMicGain(num);
  }
}

export const micGain = writable<number>(initialMicGain);

micGain.subscribe((value) => {
  if (browser) {
    localStorage.setItem(MIC_GAIN_KEY, String(value));
  }
});

// Voice activation and push-to-talk settings
export type VoiceMode = 'continuous' | 'vad' | 'ptt';

const VOICE_MODE_KEY = 'murmer_voice_mode';
const VAD_SENSITIVITY_KEY = 'murmer_vad_sensitivity';
const VAD_AUTO_KEY = 'murmer_vad_auto';
const PTT_KEY_KEY = 'murmer_ptt_key';

let initialVoiceMode: VoiceMode = 'continuous';
let initialVadSensitivity = 0.1; // 0-1 range, lower = more sensitive
// On by default: the threshold that works in one room with one microphone is
// wrong in the next, so deriving it from the measured noise floor beats asking
// for a number. `vadSensitivity` stays the manual override.
let initialVadAutoSensitivity = true;
let initialPttKey = 'Space';

if (browser) {
  const storedMode = localStorage.getItem(VOICE_MODE_KEY) as VoiceMode;
  if (storedMode && ['continuous', 'vad', 'ptt'].includes(storedMode)) {
    initialVoiceMode = storedMode;
  }
  
  const storedSensitivity = localStorage.getItem(VAD_SENSITIVITY_KEY);
  if (storedSensitivity !== null) {
    const num = parseFloat(storedSensitivity);
    if (!isNaN(num) && num >= 0 && num <= 1) {
      initialVadSensitivity = num;
    }
  }
  
  const storedVadAuto = localStorage.getItem(VAD_AUTO_KEY);
  if (storedVadAuto !== null) {
    initialVadAutoSensitivity = storedVadAuto === 'true';
  }

  const storedPttKey = localStorage.getItem(PTT_KEY_KEY);
  if (storedPttKey) {
    initialPttKey = storedPttKey;
  }
}

export const voiceMode = writable<VoiceMode>(initialVoiceMode);
export const vadSensitivity = writable<number>(initialVadSensitivity);
export const vadAutoSensitivity = writable<boolean>(initialVadAutoSensitivity);
export const pttKey = writable<string>(initialPttKey);
export const isPttActive = writable<boolean>(false);
export const voiceActivity = writable<boolean>(false);

voiceMode.subscribe((value) => {
  if (browser) {
    localStorage.setItem(VOICE_MODE_KEY, value);
  }
});

vadSensitivity.subscribe((value) => {
  if (browser) {
    localStorage.setItem(VAD_SENSITIVITY_KEY, String(value));
  }
});

vadAutoSensitivity.subscribe((value) => {
  if (browser) {
    localStorage.setItem(VAD_AUTO_KEY, String(value));
  }
});

pttKey.subscribe((value) => {
  if (browser) {
    localStorage.setItem(PTT_KEY_KEY, value);
  }
});
