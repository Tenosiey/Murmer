import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';

/**
 * Local, per-listener soundboard preferences. None of this is ever sent to the
 * server: playback happens locally on every client, so muting a sound or a
 * person only has to convince your own audio graph.
 *
 * Three independent layers, most specific last:
 *  1. master  – one volume plus a hard on/off switch (global, all servers)
 *  2. sound   – per-sound volume and mute
 *  3. user    – silence one person's sounds entirely
 *
 * Layers 2 and 3 are namespaced by server URL: sound ids and usernames are only
 * unique per server, so a flat map would bleed settings between servers.
 */

const ENABLED_KEY = 'murmer_soundboard_enabled';
const VOLUME_KEY = 'murmer_soundboard_volume';
const PREFS_KEY = 'murmer_soundboard_prefs';

/** Per-server preferences. */
interface ServerPrefs {
  /** soundId -> volume multiplier 0..1. Absent means 1.0. */
  soundVolumes: Record<number, number>;
  /** Muted sound ids. */
  mutedSounds: number[];
  /** Usernames whose sounds are never played. */
  mutedUsers: string[];
}

type PersistedPrefs = Record<string, ServerPrefs>;

function emptyPrefs(): ServerPrefs {
  return { soundVolumes: {}, mutedSounds: [], mutedUsers: [] };
}

function clampVolume(value: unknown): number {
  const num = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(num)) return 1;
  return Math.min(Math.max(num, 0), 1);
}

function loadBoolean(key: string, fallback: boolean): boolean {
  if (!browser) return fallback;
  const raw = localStorage.getItem(key);
  return raw === null ? fallback : raw === 'true';
}

function loadVolume(): number {
  if (!browser) return 1;
  const raw = localStorage.getItem(VOLUME_KEY);
  return raw === null ? 1 : clampVolume(parseFloat(raw));
}

/** Parse one server's slice defensively; malformed entries are dropped. */
function parseServerPrefs(value: unknown): ServerPrefs {
  if (!value || typeof value !== 'object') return emptyPrefs();
  const raw = value as Record<string, unknown>;
  const prefs = emptyPrefs();

  if (raw.soundVolumes && typeof raw.soundVolumes === 'object') {
    for (const [key, entry] of Object.entries(raw.soundVolumes as Record<string, unknown>)) {
      const id = Number(key);
      if (!Number.isFinite(id)) continue;
      prefs.soundVolumes[id] = clampVolume(entry);
    }
  }
  if (Array.isArray(raw.mutedSounds)) {
    prefs.mutedSounds = raw.mutedSounds.filter(
      (id): id is number => typeof id === 'number' && Number.isFinite(id)
    );
  }
  if (Array.isArray(raw.mutedUsers)) {
    prefs.mutedUsers = raw.mutedUsers.filter((u): u is string => typeof u === 'string' && !!u);
  }
  return prefs;
}

function loadPersisted(): PersistedPrefs {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: PersistedPrefs = {};
    for (const [server, prefs] of Object.entries(parsed)) {
      result[server] = parseServerPrefs(prefs);
    }
    return result;
  } catch (error) {
    console.error('Failed to parse soundboard settings', error);
    return {};
  }
}

/** Master on/off switch: when false nothing is ever played. */
export const soundboardEnabled = writable<boolean>(loadBoolean(ENABLED_KEY, true));
/** Master volume multiplier applied on top of every per-sound volume. */
export const soundboardVolume = writable<number>(loadVolume());

if (browser) {
  soundboardEnabled.subscribe((value) => localStorage.setItem(ENABLED_KEY, String(value)));
  soundboardVolume.subscribe((value) => localStorage.setItem(VOLUME_KEY, String(value)));
}

function createPrefsStore() {
  const store = writable<ServerPrefs>(emptyPrefs());
  let activeServer: string | null = null;
  const persisted = loadPersisted();

  store.subscribe((value) => {
    if (!browser || !activeServer) return;
    persisted[activeServer] = value;
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(persisted));
    } catch (error) {
      console.error('Failed to persist soundboard settings', error);
    }
  });

  return {
    subscribe: store.subscribe,
    /** Switch to the given server's persisted preferences. Called on connect. */
    setServer(url: string) {
      activeServer = url;
      store.set(persisted[url] ?? emptyPrefs());
    },
    setSoundVolume(soundId: number, volume: number) {
      store.update((current) => {
        const soundVolumes = { ...current.soundVolumes };
        const clamped = clampVolume(volume);
        // 1.0 is the default; storing it would just grow the payload.
        if (clamped === 1) delete soundVolumes[soundId];
        else soundVolumes[soundId] = clamped;
        return { ...current, soundVolumes };
      });
    },
    setSoundMuted(soundId: number, muted: boolean) {
      store.update((current) => {
        const isMuted = current.mutedSounds.includes(soundId);
        if (isMuted === muted) return current;
        return {
          ...current,
          mutedSounds: muted
            ? [...current.mutedSounds, soundId]
            : current.mutedSounds.filter((id) => id !== soundId)
        };
      });
    },
    setUserMuted(user: string, muted: boolean) {
      store.update((current) => {
        const isMuted = current.mutedUsers.includes(user);
        if (isMuted === muted) return current;
        return {
          ...current,
          mutedUsers: muted
            ? [...current.mutedUsers, user]
            : current.mutedUsers.filter((u) => u !== user)
        };
      });
    }
  };
}

export const soundboardPrefs = createPrefsStore();

/**
 * The gain to play `soundId` from `user` at, or `null` when it must not be
 * played at all. Single source of truth for the three mute/volume layers so the
 * player and the UI can never disagree.
 */
export function effectiveGain(soundId: number, user: string): number | null {
  if (!get(soundboardEnabled)) return null;
  const prefs = get(soundboardPrefs);
  if (prefs.mutedUsers.includes(user)) return null;
  if (prefs.mutedSounds.includes(soundId)) return null;
  const gain = get(soundboardVolume) * (prefs.soundVolumes[soundId] ?? 1);
  return gain > 0 ? gain : null;
}
