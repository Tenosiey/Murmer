import { writable } from 'svelte/store';
import { chat } from './chat';
import { microphoneMuted } from './settings';
import type { Message } from '../types';

/**
 * Whether the current user may talk (Talk = Send) in the voice channel they are
 * in. Voice audio is peer-to-peer, so the server cannot mute a client mid-call;
 * it sends this hint on join and the client enforces listen-only by keeping the
 * microphone muted and disabling the mic control. A modified client could
 * bypass this — View/join is the server-enforced boundary.
 */
export const canSpeak = writable<boolean>(true);

chat.on('voice-permissions', (msg: Message) => {
  const allowed = (msg as any).canSpeak !== false;
  canSpeak.set(allowed);
  // Force listen-only: mute the mic the moment a no-talk channel is joined.
  if (!allowed) microphoneMuted.set(true);
});

/** Reset to the default (allowed) when leaving voice. */
export function resetVoicePermissions() {
  canSpeak.set(true);
}
