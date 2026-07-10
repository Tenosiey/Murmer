/**
 * Tracks the microphone/output mute state of remote users in voice channels so
 * the sidebar can show a small indicator beside muted users.
 *
 * State is keyed by username (usernames are unique server-wide). Entries are
 * only ever displayed for users currently listed in a voice channel, so stale
 * entries left behind after someone leaves are harmless and get overwritten
 * when they rejoin.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

export interface MuteState {
  micMuted: boolean;
  outputMuted: boolean;
}

function createVoiceMuteStore() {
  const { subscribe, update } = writable<Record<string, MuteState>>({});

  chat.on('voice-mute', (msg: Message) => {
    const user = (msg as any).user as string;
    if (!user) return;
    const state: MuteState = {
      micMuted: Boolean((msg as any).micMuted),
      outputMuted: Boolean((msg as any).outputMuted)
    };
    update((m) => ({ ...m, [user]: state }));
  });

  // Snapshot of everyone already muted in a channel, sent when we join.
  chat.on('voice-mute-active', (msg: Message) => {
    const states = (msg as any).states;
    if (!states || typeof states !== 'object') return;
    update((m) => {
      const next = { ...m };
      for (const [user, value] of Object.entries(states)) {
        next[user] = {
          micMuted: Boolean((value as any).micMuted),
          outputMuted: Boolean((value as any).outputMuted)
        };
      }
      return next;
    });
  });

  return { subscribe };
}

export const voiceMuteStates = createVoiceMuteStore();
