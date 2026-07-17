import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, VoiceChannelInfo } from '../types';

function normalizeChannel(value: any): VoiceChannelInfo | null {
  if (!value || typeof value !== 'object') return null;
  const id = typeof value.id === 'number' ? value.id : null;
  if (id === null) return null;
  const name =
    typeof value.name === 'string'
      ? value.name
      : typeof value.channel === 'string'
        ? value.channel
        : null;
  if (!name) return null;
  const quality =
    typeof value.quality === 'string' && value.quality.trim() ? value.quality.trim() : 'standard';
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
  const position = typeof value.position === 'number' ? value.position : 0;
  return { id, name, quality, bitrate, categoryId, position };
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
    const raw = msg as any;
    const info = normalizeChannel({ ...raw, id: raw.channelId ?? raw.id });
    if (info) {
      update((chs) => {
        const existing = chs.find((c) => c.id === info.id);
        return existing ? chs : [...chs, info];
      });
    }
  });

  chat.on('voice-channel-update', (msg: Message) => {
    const raw = msg as any;
    const info = normalizeChannel({ ...raw, id: raw.channelId ?? raw.id });
    if (info) {
      update((chs) =>
        chs.map((c) =>
          c.id === info.id ? { ...c, quality: info.quality, bitrate: info.bitrate } : c
        )
      );
    }
  });

  chat.on('voice-channel-remove', (msg: Message) => {
    const id = (msg as any).channelId;
    if (typeof id === 'number') {
      update((chs) => chs.filter((c) => c.id !== id));
    }
  });

  chat.on('channel-move', (msg: Message) => {
    const raw = msg as any;
    if (raw.voice !== true) return;
    const id = typeof raw.channelId === 'number' ? raw.channelId : null;
    if (id === null) return;
    const categoryId = typeof raw.categoryId === 'number' ? raw.categoryId : null;
    update((chs) =>
      chs.map((c) =>
        c.id === id
          ? { ...c, categoryId, position: typeof raw.position === 'number' ? raw.position : c.position }
          : c
      )
    );
  });

  chat.on('channel-reorder', (msg: Message) => {
    const raw = msg as any;
    if (raw.voice !== true) return;
    if (!Array.isArray(raw.order)) return;
    const categoryId = typeof raw.categoryId === 'number' ? raw.categoryId : null;
    const positions = new Map<number, number>(
      raw.order
        .filter((id: any): id is number => typeof id === 'number')
        .map((id: number, index: number) => [id, index])
    );
    update((chs) =>
      chs.map((c) =>
        positions.has(c.id) ? { ...c, categoryId, position: positions.get(c.id)! } : c
      )
    );
  });

  function create(
    name: string,
    preset?: Pick<VoiceChannelInfo, 'quality' | 'bitrate'>,
    categoryId?: number | null
  ) {
    const payload: Record<string, unknown> = { type: 'create-voice-channel', name };
    if (preset) {
      payload.quality = preset.quality;
      payload.bitrate = preset.bitrate ?? null;
    }
    if (categoryId != null) payload.categoryId = categoryId;
    chat.sendRaw(payload);
  }

  function configure(channelId: number, preset: Pick<VoiceChannelInfo, 'quality' | 'bitrate'>) {
    const payload: Record<string, unknown> = {
      type: 'update-voice-channel',
      channelId,
      quality: preset.quality,
      bitrate: preset.bitrate ?? null
    };
    chat.sendRaw(payload);
  }

  function remove(channelId: number) {
    chat.sendRaw({ type: 'delete-voice-channel', channelId });
  }

  return { subscribe, set, create, configure, remove };
}

export const voiceChannels = createVoiceChannelStore();
