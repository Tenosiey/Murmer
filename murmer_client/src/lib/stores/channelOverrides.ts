import { writable } from 'svelte/store';
import { chat } from './chat';
import type { ChannelOverride, Message } from '../types';

export interface ChannelOverridesEntry {
  channelId: number;
  voice: boolean;
  overrides: ChannelOverride[];
}

/** Cache key for a channel's overrides (text and voice ids can collide). */
export function overridesKey(voice: boolean, channelId: number): string {
  return `${voice ? 'v' : 't'}:${channelId}`;
}

function parseOverride(raw: unknown): ChannelOverride | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Record<string, unknown>;
  const type = r.targetType;
  if (type !== 'everyone' && type !== 'role' && type !== 'user') return null;
  return {
    targetType: type,
    targetId: typeof r.targetId === 'string' ? r.targetId : '',
    targetLabel: typeof r.targetLabel === 'string' ? r.targetLabel : '',
    allow: typeof r.allow === 'number' ? r.allow : 0,
    deny: typeof r.deny === 'number' ? r.deny : 0
  };
}

/**
 * Per-channel permission overrides, only ever populated for users who can
 * manage channels (the server sends `channel-overrides` on request/change).
 * Keyed by {@link overridesKey}.
 */
function createChannelOverridesStore() {
  const { subscribe, update, set } = writable<Record<string, ChannelOverridesEntry>>({});

  chat.on('channel-overrides', (msg: Message) => {
    const raw = msg as any;
    const channelId = typeof raw.channelId === 'number' ? raw.channelId : null;
    if (channelId === null) return;
    const voice = raw.voice === true;
    const overrides = Array.isArray(raw.overrides)
      ? raw.overrides.map(parseOverride).filter((o: ChannelOverride | null): o is ChannelOverride => o !== null)
      : [];
    update((m) => ({ ...m, [overridesKey(voice, channelId)]: { channelId, voice, overrides } }));
  });

  /** Ask the server for a channel's overrides (manager only). */
  function request(channelId: number, voice: boolean) {
    chat.sendRaw({ type: 'get-channel-overrides', channelId, voice });
  }

  function setOverride(
    channelId: number,
    voice: boolean,
    target: { type: 'everyone' } | { type: 'role'; id: number } | { type: 'user'; user: string },
    allow: number,
    deny: number
  ) {
    chat.sendRaw({ type: 'set-channel-override', channelId, voice, target, allow, deny });
  }

  function removeOverride(
    channelId: number,
    voice: boolean,
    target: { type: 'everyone' } | { type: 'role'; id: number } | { type: 'user'; user: string }
  ) {
    chat.sendRaw({ type: 'remove-channel-override', channelId, voice, target });
  }

  return { subscribe, reset: () => set({}), request, setOverride, removeOverride };
}

export const channelOverrides = createChannelOverridesStore();
