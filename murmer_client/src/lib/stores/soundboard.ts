import { derived, writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message, SoundboardSound } from '../types';

/**
 * The server's shared soundboard library. The server pushes the full list as a
 * `sound-list` frame after authentication and re-broadcasts it whenever a
 * manager adds, renames or deletes a sound, so this store never has to request
 * anything.
 *
 * URLs stay relative (`/files/...`) and are resolved against the selected
 * server when a sound is played.
 */
function createSoundboardStore() {
  const { subscribe, set } = writable<SoundboardSound[]>([]);

  chat.on('sound-list', (msg: Message) => {
    const raw = (msg as any).sounds;
    if (!Array.isArray(raw)) return;
    const sounds: SoundboardSound[] = [];
    for (const entry of raw) {
      if (!entry || typeof entry !== 'object') continue;
      const id = entry.id;
      const name = typeof entry.name === 'string' ? entry.name.trim() : '';
      const url = typeof entry.url === 'string' ? entry.url : '';
      // Validate server data before acting on it: only well-formed entries
      // with a relative upload URL make it into client state, so a tampered
      // frame can never point playback at an arbitrary host.
      if (typeof id !== 'number' || !Number.isFinite(id)) continue;
      if (!name || !url.startsWith('/files/')) continue;
      sounds.push({
        id,
        name,
        url,
        uploadedBy: typeof entry.uploadedBy === 'string' ? entry.uploadedBy : undefined,
        playCount: typeof entry.playCount === 'number' ? entry.playCount : 0,
        createdAt: typeof entry.createdAt === 'string' ? entry.createdAt : undefined
      });
    }
    set(sounds);
  });

  connection.subscribe((state) => {
    // A fresh list arrives with the next successful auth.
    if (state !== 'connected') set([]);
  });

  return { subscribe };
}

export const soundboardSounds = createSoundboardStore();

/** Sounds sorted by play count (most played first), for the picker's default order. */
export const soundsByPopularity = derived(soundboardSounds, ($sounds) =>
  [...$sounds].sort((a, b) => b.playCount - a.playCount || a.name.localeCompare(b.name))
);
