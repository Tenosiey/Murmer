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

let initialUserVolumes: Record<string, number> = {};
if (browser) {
  const stored = localStorage.getItem(USER_VOLUMES_KEY);
  if (stored) {
    try {
      initialUserVolumes = JSON.parse(stored);
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
    [userId]: Math.max(0, Math.min(1, volume))
  }));
}

export function getUserVolume(userId: string): number {
  let currentVolumes: Record<string, number> = {};
  userVolumes.subscribe(volumes => currentVolumes = volumes)();
  return currentVolumes[userId] ?? 1.0;
}

// Voice activation and push-to-talk settings
export type VoiceMode = 'continuous' | 'vad' | 'ptt';

const VOICE_MODE_KEY = 'murmer_voice_mode';
const VAD_SENSITIVITY_KEY = 'murmer_vad_sensitivity';
const PTT_KEY_KEY = 'murmer_ptt_key';

let initialVoiceMode: VoiceMode = 'continuous';
let initialVadSensitivity = 0.1; // 0-1 range, lower = more sensitive
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
  
  const storedPttKey = localStorage.getItem(PTT_KEY_KEY);
  if (storedPttKey) {
    initialPttKey = storedPttKey;
  }
}

export const voiceMode = writable<VoiceMode>(initialVoiceMode);
export const vadSensitivity = writable<number>(initialVadSensitivity);
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

pttKey.subscribe((value) => {
  if (browser) {
    localStorage.setItem(PTT_KEY_KEY, value);
  }
});
