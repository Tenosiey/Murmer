import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createPingStore() {
  const { subscribe, set } = writable(0);
  let currentId: number | null = null;
  let interval: number | null = null;

  function sendPing() {
    if (currentId !== null) return;
    currentId = Date.now();
    chat.sendRaw({ type: 'ping', id: currentId });
  }

  function handlePong(msg: Message) {
    if (typeof msg.id === 'number' && msg.id === currentId) {
      set(Date.now() - msg.id);
      currentId = null;
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
  }

  return { subscribe, start, stop };
}

export const ping = createPingStore();
