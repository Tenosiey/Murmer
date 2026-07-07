import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createPingStore() {
  const { subscribe, set } = writable(0);
  let currentId: number | null = null;
  let lastSentAt: number | null = null;
  let unansweredSince: number | null = null;
  let interval: number | null = null;

  const STALE_PING_MS = 10_000;
  /** No pong for this long means the connection is dead even if the socket looks open. */
  const DEAD_CONNECTION_MS = 15_000;

  function sendPing() {
    const now = Date.now();

    if (unansweredSince !== null && now - unansweredSince >= DEAD_CONNECTION_MS) {
      chat.connectionLost();
      return;
    }

    if (currentId !== null) {
      if (lastSentAt && now - lastSentAt < STALE_PING_MS) {
        return;
      }

      if (lastSentAt) {
        set(now - lastSentAt);
      }
    }

    if (unansweredSince === null) {
      unansweredSince = now;
    }
    currentId = now;
    lastSentAt = now;
    chat.sendRaw({ type: 'ping', id: currentId });
  }

  function handlePong(msg: Message) {
    // Any pong, even a late one for an older ping, proves the link is alive.
    unansweredSince = null;
    if (typeof msg.id === 'number' && msg.id === currentId) {
      set(Date.now() - msg.id);
      currentId = null;
      lastSentAt = null;
    }
  }

  function start() {
    stop();
    chat.on('pong', handlePong);
    sendPing();
    interval = window.setInterval(sendPing, 5000);
  }

  function stop() {
    if (interval !== null) {
      clearInterval(interval);
      interval = null;
    }
    chat.off('pong');
    currentId = null;
    lastSentAt = null;
    unansweredSince = null;
    set(0);
  }

  return { subscribe, start, stop };
}

export const ping = createPingStore();
