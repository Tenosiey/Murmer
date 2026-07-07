import { writable } from 'svelte/store';
import type { Message } from '../types';

export interface ThreadData {
  rootId: number;
  channelId: number;
  messages: Message[];
}

/** Server response for the most recently requested thread. */
export const threadData = writable<ThreadData | null>(null);
