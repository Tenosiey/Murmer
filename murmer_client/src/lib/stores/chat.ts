import { writable } from 'svelte/store';

export interface Message {
  type: string;
  user: string;
  text?: string;
  [key: string]: unknown;
}

function createChatStore() {
  const { subscribe, update, set } = writable<Message[]>([]);
  let socket: WebSocket | null = null;
  const handlers: Record<string, (msg: Message) => void> = {};

  function on(type: string, cb: (msg: Message) => void) {
    handlers[type] = cb;
  }

  function off(type: string) {
    delete handlers[type];
  }

  function connect(url: string, onOpen?: () => void) {
    if (socket) return;
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
          update((m) => [...m, msg]);
        } else if (msg.type && handlers[msg.type]) {
          handlers[msg.type](msg);
        }
      } catch (_) {}
    });
    socket.addEventListener('close', () => {
      if (import.meta.env.DEV) console.log('WebSocket connection closed');
      socket = null;
    });
    socket.addEventListener('error', (e) => {
      if (import.meta.env.DEV) console.error('WebSocket error', e);
    });
  }

  function send(user: string, text: string) {
    if (socket && socket.readyState === WebSocket.OPEN) {
      if (import.meta.env.DEV) console.log('Sending:', { user, text });
      socket.send(JSON.stringify({ type: 'chat', user, text }));
    }
  }

  function sendRaw(data: any) {
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(data));
    }
  }

  return { subscribe, connect, send, sendRaw, on, off, clear: () => set([]) };
}

export const chat = createChatStore();
