import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createVoiceUserStore() {
  const { subscribe, update } = writable<Record<string, string[]>>({});
  chat.on('voice-users', (msg: Message) => {
    const ch = (msg as any).channel;
    if (typeof ch === 'string' && Array.isArray(msg.users)) {
      update((m) => ({ ...m, [ch]: msg.users as string[] }));
    }
  });
  return { subscribe };
}

export const voiceUsers = createVoiceUserStore();
