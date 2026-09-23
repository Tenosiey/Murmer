/**
 * The ICE servers every WebRTC peer connection is built with, as announced
 * by the server after authentication (`STUN_SERVERS` on the operator's side).
 *
 * The client keeps no default of its own: a URL compiled in here would be
 * contacted on every call regardless of what the operator chose, which is
 * exactly what this store exists to stop.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message } from '../types';

export const iceServers = writable<RTCIceServer[]>([]);

/** Keep only well-formed STUN entries; anything else is dropped. */
export function parseIceServers(raw: unknown): RTCIceServer[] {
  if (!Array.isArray(raw)) return [];
  const urls = raw
    .map((entry) => (typeof entry === 'object' && entry !== null ? entry.urls : undefined))
    .filter((url): url is string => typeof url === 'string' && /^stuns?:/.test(url));
  return urls.map((url) => ({ urls: url }));
}

chat.on('ice-config', (msg: Message) => {
  iceServers.set(parseIceServers(msg.iceServers));
});

connection.subscribe((state) => {
  // Forget the previous server's servers; the next one announces its own.
  if (state !== 'connected') iceServers.set([]);
});
