import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, VoiceChannelInfo } from '../types';

function normalizeChannel(value: any): VoiceChannelInfo | null {
  if (!value || typeof value !== 'object') return null;
  const name = typeof value.name === 'string' ? value.name : typeof value.channel === 'string' ? value.channel : null;
  if (!name) return null;
  const quality = typeof value.quality === 'string' && value.quality.trim() ? value.quality.trim() : 'standard';
  let bitrate: number | null = null;
  if (value.bitrate === null || value.bitrate === undefined) {
    bitrate = null;
  } else if (typeof value.bitrate === 'number' && Number.isFinite(value.bitrate)) {
    bitrate = Math.max(0, Math.round(value.bitrate));
  } else if (typeof value.bitrate === 'string') {
    const parsed = Number(value.bitrate);
    if (!Number.isNaN(parsed) && Number.isFinite(parsed)) {
      bitrate = Math.max(0, Math.round(parsed));
    }
  }
  const categoryId = typeof value.categoryId === 'number' ? value.categoryId : null;
  return { name, quality, bitrate, categoryId };
}

function createVoiceChannelStore() {
  const { subscribe, set, update } = writable<VoiceChannelInfo[]>([]);

  chat.on('voice-channel-list', (msg: Message) => {
    const list = (msg as any).channels;
    if (Array.isArray(list)) {
      const items = list
        .map((item) => normalizeChannel(item))
        .filter((item): item is VoiceChannelInfo => Boolean(item));
      set(items);
    }
  });

  chat.on('voice-channel-add', (msg: Message) => {
    const info = normalizeChannel(msg as any);
    if (info) {
      update((chs) => {
        const existing = chs.find((c) => c.name === info.name);
        return existing ? chs : [...chs, info];
      });
    }
  });

  chat.on('voice-channel-update', (msg: Message) => {
    const info = normalizeChannel(msg as any);
    if (info) {
      update((chs) =>
        chs.map((c) => (c.name === info.name ? { ...c, quality: info.quality, bitrate: info.bitrate } : c))
      );
    }
  });

  chat.on('voice-channel-remove', (msg: Message) => {
    const name = (msg as any).channel;
    if (typeof name === 'string') {
      update((chs) => chs.filter((c) => c.name !== name));
    }
  });

  chat.on('channel-move', (msg: Message) => {
    const raw = msg as any;
    if (raw.voice !== true) return;
    const name = typeof raw.channel === 'string' ? raw.channel : null;
    if (!name) return;
    const categoryId = typeof raw.categoryId === 'number' ? raw.categoryId : null;
    update((chs) => chs.map((c) => (c.name === name ? { ...c, categoryId } : c)));
  });

  function create(name: string, preset?: Pick<VoiceChannelInfo, 'quality' | 'bitrate'>, categoryId?: number | null) {
    const payload: Record<string, unknown> = { type: 'create-voice-channel', name };
    if (preset) {
      payload.quality = preset.quality;
      payload.bitrate = preset.bitrate ?? null;
    }
    if (categoryId != null) payload.categoryId = categoryId;
    chat.sendRaw(payload);
  }

  function configure(name: string, preset: Pick<VoiceChannelInfo, 'quality' | 'bitrate'>) {
    const payload: Record<string, unknown> = {
      type: 'update-voice-channel',
      name,
      quality: preset.quality,
      bitrate: preset.bitrate ?? null
    };
    chat.sendRaw(payload);
  }

  function remove(name: string) {
    chat.sendRaw({ type: 'delete-voice-channel', name });
  }

  return { subscribe, set, create, configure, remove };
}

export const voiceChannels = createVoiceChannelStore();
