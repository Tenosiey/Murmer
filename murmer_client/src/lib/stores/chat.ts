import { writable } from 'svelte/store';

export interface Message {
  user: string;
  text: string;
}

function createChatStore() {
  const { subscribe, update, set } = writable<Message[]>([]);
  let socket: WebSocket | null = null;

  function connect(url: string) {
    if (socket) return;
    if (import.meta.env.DEV) console.log('Connecting to WebSocket', url);
    socket = new WebSocket(url);
    socket.addEventListener('open', () => {
      if (import.meta.env.DEV) console.log('WebSocket connection opened');
    });
    socket.addEventListener('message', (ev) => {
      if (import.meta.env.DEV) console.log('Received:', ev.data);
      try {
        const msg: Message = JSON.parse(ev.data);
        update((m) => [...m, msg]);
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
      socket.send(JSON.stringify({ user, text }));
    }
  }

  return { subscribe, connect, send, clear: () => set([]) };
}

export const chat = createChatStore();
