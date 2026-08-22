/**
 * The quality preset and bitrate new voice channels start with, configured
 * server-wide in the Server Dashboard.
 *
 * These are defaults, not a cap: the create-channel dialog preselects them
 * and the server applies them when a client names nothing. An existing
 * channel keeps whatever it was created with.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import { DEFAULT_VOICE_PRESET, MAX_VOICE_BITRATE } from '../chat/constants';
import type { Message } from '../types';

export interface VoiceDefaults {
  /** Quality label, matching one of `VOICE_QUALITY_PRESETS` on a stock server. */
  quality: string;
  /** Bits per second, or null for an uncompressed ("lossless") channel. */
  bitrate: number | null;
}

function defaults(): VoiceDefaults {
  return { quality: DEFAULT_VOICE_PRESET.quality, bitrate: DEFAULT_VOICE_PRESET.bitrate };
}

export const voiceDefaults = writable<VoiceDefaults>(defaults());

chat.on('voice-defaults', (msg: Message) => {
  const payload = msg as any;
  const quality =
    typeof payload.quality === 'string' && payload.quality.trim()
      ? payload.quality.trim()
      : DEFAULT_VOICE_PRESET.quality;
  // `null` is a real value here (lossless), so it must survive the parse; a
  // nonsensical number falls back rather than being handed to the encoder.
  const bitrate =
    payload.bitrate === null
      ? null
      : typeof payload.bitrate === 'number' &&
          Number.isFinite(payload.bitrate) &&
          payload.bitrate > 0
        ? Math.min(Math.round(payload.bitrate), MAX_VOICE_BITRATE)
        : DEFAULT_VOICE_PRESET.bitrate;
  voiceDefaults.set({ quality, bitrate });
});

connection.subscribe((state) => {
  if (state !== 'connected') voiceDefaults.set(defaults());
});

/**
 * Set the server-wide voice defaults (requires Manage Server; enforced
 * server-side). The confirmation arrives as a broadcast `voice-defaults`
 * frame which updates this store.
 */
export function setVoiceDefaults(quality: string, bitrate: number | null): void {
  chat.sendRaw({ type: 'set-voice-defaults', quality, bitrate });
}
