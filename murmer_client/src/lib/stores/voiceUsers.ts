import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createVoiceUserStore() {
  const { subscribe, update } = writable<Record<number, string[]>>({});
  chat.on('voice-users', (msg: Message) => {
    const chId = (msg as any).channelId;
    if (typeof chId === 'number' && Array.isArray(msg.users)) {
      update((m) => ({ ...m, [chId]: msg.users as string[] }));
    }
  });
  return { subscribe };
}

export const voiceUsers = createVoiceUserStore();
