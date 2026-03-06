import { writable } from 'svelte/store';

const HOLD_MS = 500;

const initial: Record<string, boolean> = {};

export const remoteSpeaking = writable<Record<string, boolean>>(initial);

const releaseTimers = new Map<string, ReturnType<typeof setTimeout>>();

export function setRemoteSpeaking(userId: string, speaking: boolean) {
  if (speaking) {
    const pending = releaseTimers.get(userId);
    if (pending !== undefined) {
      clearTimeout(pending);
      releaseTimers.delete(userId);
    }

    remoteSpeaking.update((current) => {
      if (current[userId]) return current;
      return { ...current, [userId]: true };
    });
  } else {
    if (releaseTimers.has(userId)) return;

    releaseTimers.set(
      userId,
      setTimeout(() => {
        releaseTimers.delete(userId);
        remoteSpeaking.update((current) => {
          if (!current[userId]) return current;
          const next = { ...current };
          delete next[userId];
          return next;
        });
      }, HOLD_MS)
    );
  }
}

export function resetRemoteSpeaking() {
  for (const timer of releaseTimers.values()) {
    clearTimeout(timer);
  }
  releaseTimers.clear();
  remoteSpeaking.set({});
}
