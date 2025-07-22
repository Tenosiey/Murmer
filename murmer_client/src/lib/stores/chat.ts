import { writable, get } from 'svelte/store';
import type { Message } from '../types';
import { session } from './session';
import { notify } from '../notify';

function createChatStore() {
  const { subscribe, update, set } = writable<Message[]>([]);
  let socket: WebSocket | null = null;
  let currentUrl: string | null = null;
  const handlers: Record<string, Array<(msg: Message) => void>> = {};

  function on(type: string, cb: (msg: Message) => void) {
    if (!handlers[type]) {
      handlers[type] = [];
    }
    handlers[type].push(cb);
  }

  function off(type: string, cb?: (msg: Message) => void) {
    if (!handlers[type]) return;
    if (cb) {
      handlers[type] = handlers[type].filter((h) => h !== cb);
      if (handlers[type].length === 0) {
        delete handlers[type];
      }
    } else {
      delete handlers[type];
    }
  }

  function connect(url: string, onOpen?: () => void) {
    if (socket) {
      if (currentUrl === url) return;
      socket.close();
      socket = null;
    }
    set([]); // clear previous history when connecting to a server
    currentUrl = url;
    if (import.meta.env.DEV) console.log('Connecting to WebSocket', url);
    socket = new WebSocket(url);
    socket.addEventListener('open', () => {
      if (import.meta.env.DEV) console.log('WebSocket connection opened');
      if (onOpen) onOpen();
    });
    socket.addEventListener('message', (ev) => {
      if (import.meta.env.DEV) console.log('Received:', ev.data);
      try {
        const msg: Message = JSON.parse(ev.data);
        if (msg.type === 'chat') {
          if (!msg.time) msg.time = new Date().toLocaleTimeString();
          update((m) => [...m, msg]);
          const current = get(session).user;
          if (!current || msg.user !== current) {
            notify('New message', `${msg.user}: ${msg.text ?? ''}`);
          }
        } else if (msg.type && handlers[msg.type]) {
          for (const handler of handlers[msg.type]) {
            handler(msg);
          }
        }
      } catch (_) {}
    });
    socket.addEventListener('close', () => {
      if (import.meta.env.DEV) console.log('WebSocket connection closed');
      socket = null;
      currentUrl = null;
    });
    socket.addEventListener('error', (e) => {
      if (import.meta.env.DEV) console.error('WebSocket error', e);
    });
  }

  function send(user: string, text: string) {
    if (socket && socket.readyState === WebSocket.OPEN) {
      const payload = { type: 'chat', user, text, time: new Date().toLocaleTimeString() };
      if (import.meta.env.DEV) console.log('Sending:', payload);
      socket.send(JSON.stringify(payload));
    }
  }

  function sendRaw(data: any) {
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(data));
    }
  }

  function disconnect() {
    if (socket) {
      socket.close();
      // 'close' event handler will reset state
    }
    set([]); // clear chat history on disconnect
  }

  return { subscribe, connect, send, sendRaw, on, off, disconnect, clear: () => set([]) };
}

export const chat = createChatStore();
