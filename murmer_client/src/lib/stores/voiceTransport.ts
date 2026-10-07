import { writable } from 'svelte/store';

/**
 * Set while our voice call runs through the server's SFU rather than peer to
 * peer, with the round-trip time to the server; `null` on the mesh or out of
 * voice. The user chose a peer-to-peer app, so the UI says plainly when the
 * server can see their media. Written only by `voice/manager.ts`.
 */
export const viaServer = writable<{ rtt: number } | null>(null);
