import { writable } from 'svelte/store';

/**
 * Lifecycle state of the chat server connection.
 * - `idle`: no connection attempt in progress (e.g. before joining a server)
 * - `connecting`: WebSocket handshake in progress
 * - `connected`: connection established
 * - `disconnected`: an established connection was lost
 * - `failed`: the connection could not be established at all
 */
export type ConnectionState = 'idle' | 'connecting' | 'connected' | 'disconnected' | 'failed';

/** Current chat WebSocket connection state, maintained by the chat store. */
export const connection = writable<ConnectionState>('idle');

/**
 * Error to surface on the servers page after the user is sent back there
 * (e.g. wrong password, banned). Consumed once and cleared by the page.
 */
export const connectionError = writable<string | null>(null);
