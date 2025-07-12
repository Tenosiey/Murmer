import { writable, derived, get } from 'svelte/store';
import type { RemotePeer, ConnectionStats } from '../voice/manager';
import { VoiceManager } from '../voice/manager';

const peers = writable<RemotePeer[]>([]);
const manager = new VoiceManager();
manager.subscribe((list) => peers.set(list));

export const voice = {
  subscribe: peers.subscribe,
  join: (user: string) => manager.join(user, get(peers)),
  leave: () => manager.leave(get(peers))
};

export const voiceStats = derived(voice, ($voice) => {
  const map: Record<string, ConnectionStats> = {};
  for (const p of $voice as unknown as RemotePeer[]) {
    if (p.stats) map[p.id] = p.stats;
  }
  return map;
});
