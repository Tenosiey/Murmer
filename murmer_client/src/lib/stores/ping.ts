import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createPingStore() {
  const { subscribe, set } = writable(0);
  let currentId: number | null = null;
  let lastSentAt: number | null = null;
  let interval: number | null = null;

  const STALE_PING_MS = 10_000;

  function sendPing() {
    const now = Date.now();

    if (currentId !== null) {
      if (lastSentAt && now - lastSentAt < STALE_PING_MS) {
        return;
      }

      if (lastSentAt) {
        set(now - lastSentAt);
      }
    }

    currentId = now;
    lastSentAt = now;
    chat.sendRaw({ type: 'ping', id: currentId });
  }

  function handlePong(msg: Message) {
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
    set(0);
  }

  return { subscribe, start, stop };
}

export const ping = createPingStore();
