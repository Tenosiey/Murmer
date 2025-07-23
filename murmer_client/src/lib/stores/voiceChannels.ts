import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createVoiceChannelStore() {
  const { subscribe, set, update } = writable<string[]>([]);

  chat.on('voice-channel-list', (msg: Message) => {
    const list = (msg as any).channels;
    if (Array.isArray(list)) {
      set(list as string[]);
    }
  });

  chat.on('voice-channel-add', (msg: Message) => {
    const name = (msg as any).channel;
    if (typeof name === 'string') {
      update((chs) => (chs.includes(name) ? chs : [...chs, name]));
    }
  });

  function create(name: string) {
    chat.sendRaw({ type: 'create-voice-channel', name });
  }

  return { subscribe, set, create };
}

export const voiceChannels = createVoiceChannelStore();
