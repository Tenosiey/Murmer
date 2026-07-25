import { writable, derived, get } from 'svelte/store';
import type { RemotePeer, ConnectionStats, VoiceChannelInfo } from '../types';
import { VoiceManager } from '../voice/manager';
import { SoundboardPlayer } from '../voice/soundboard';

const peers = writable<RemotePeer[]>([]);
const manager = new VoiceManager();
manager.subscribe((list) => peers.set(list));

/**
 * Plays soundboard clips locally. Its lifecycle is tied to the voice session:
 * `soundboard-play` frames are only meaningful while joined to the channel they
 * name, so the player is told which channel we are in here rather than having
 * every caller remember to.
 */
export const soundboardPlayer = new SoundboardPlayer();

export const voice = {
  subscribe: peers.subscribe,
  join: async (user: string, channelId: number, info?: VoiceChannelInfo) => {
    // Only arm the player once the microphone was actually acquired; a failed
    // join must not leave us playing sounds for a channel we are not in.
    await manager.join(user, channelId, get(peers), info);
    soundboardPlayer.setChannel(channelId);
  },
  leave: (channelId: number) => {
    soundboardPlayer.setChannel(null);
    manager.leave(channelId, get(peers));
  },
  /** Raw microphone capture of the running session, or null when not joined. */
  captureStream: () => manager.getCaptureStream()
};

export const voiceStats = derived(voice, ($voice) => {
  const map: Record<string, ConnectionStats> = {};
  for (const p of $voice as unknown as RemotePeer[]) {
    if (p.stats) map[p.id] = p.stats;
  }
  return map;
});
