import { writable, derived, get } from 'svelte/store';
import type { RemotePeer, ConnectionStats, VoiceChannelInfo } from '../types';
import { VoiceManager } from '../voice/manager';

const peers = writable<RemotePeer[]>([]);
const manager = new VoiceManager();
manager.subscribe((list) => peers.set(list));

export const voice = {
  subscribe: peers.subscribe,
  join: (user: string, channel: string, info?: VoiceChannelInfo) =>
    manager.join(user, channel, get(peers), info),
  leave: (channel: string) => manager.leave(channel, get(peers))
};

export const voiceStats = derived(voice, ($voice) => {
  const map: Record<string, ConnectionStats> = {};
  for (const p of $voice as unknown as RemotePeer[]) {
    if (p.stats) map[p.id] = p.stats;
  }
  return map;
});
