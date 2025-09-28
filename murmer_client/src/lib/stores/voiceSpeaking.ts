import { writable } from 'svelte/store';

/**
 * Reactive map of remote users that are currently speaking.
 *
 * A user is present in the map only while they are considered active to keep
 * subscriptions lightweight.
 */
const initial: Record<string, boolean> = {};

export const remoteSpeaking = writable<Record<string, boolean>>(initial);

/**
 * Update the speaking state for a remote user.
 */
export function setRemoteSpeaking(userId: string, speaking: boolean) {
  remoteSpeaking.update((current) => {
    const prev = current[userId] ?? false;
    if (prev === speaking) {
      return current;
    }

    if (speaking) {
      return { ...current, [userId]: true };
    }

    const next = { ...current };
    delete next[userId];
    return next;
  });
}

/**
 * Clear all remote speaking indicators, typically when leaving a voice
 * channel.
 */
export function resetRemoteSpeaking() {
  remoteSpeaking.set({});
}
