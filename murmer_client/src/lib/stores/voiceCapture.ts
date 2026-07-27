/**
 * The microphone capture of the running voice session, or null when not in a
 * channel.
 *
 * Lives in its own module rather than on `stores/voice.ts` so the voice
 * manager can publish to it without importing the store that owns it. The
 * settings level meter subscribes so it re-attaches when the manager swaps
 * the capture (input device change, processing toggles) instead of silently
 * metering the stream of the device the user just switched away from.
 */
import { writable } from 'svelte/store';

export const captureStream = writable<MediaStream | null>(null);
