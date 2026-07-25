import { writable } from 'svelte/store';

const HOLD_MS = 500;

/** RMS level above which a metered stream counts as "speaking". */
export const SPEAKING_RMS_THRESHOLD = 0.04;

const initial: Record<string, boolean> = {};

/**
 * Who is currently audible in the voice channel, keyed by user name. Remote
 * peers are metered from their incoming stream, the local user from the
 * post-gate outgoing stream (see `voice/manager.ts`), so the same threshold and
 * hold apply to everybody regardless of transmission mode.
 */
export const speakingUsers = writable<Record<string, boolean>>(initial);

const releaseTimers = new Map<string, ReturnType<typeof setTimeout>>();

export function setSpeaking(userId: string, speaking: boolean) {
  if (speaking) {
    const pending = releaseTimers.get(userId);
    if (pending !== undefined) {
      clearTimeout(pending);
      releaseTimers.delete(userId);
    }

    speakingUsers.update((current) => {
      if (current[userId]) return current;
      return { ...current, [userId]: true };
    });
  } else {
    if (releaseTimers.has(userId)) return;

    releaseTimers.set(
      userId,
      setTimeout(() => {
        releaseTimers.delete(userId);
        speakingUsers.update((current) => {
          if (!current[userId]) return current;
          const next = { ...current };
          delete next[userId];
          return next;
        });
      }, HOLD_MS)
    );
  }
}

export function resetSpeaking() {
  for (const timer of releaseTimers.values()) {
    clearTimeout(timer);
  }
  releaseTimers.clear();
  speakingUsers.set({});
}
