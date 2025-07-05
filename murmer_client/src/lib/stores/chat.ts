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
    socket = new WebSocket(url);
    socket.addEventListener('message', (ev) => {
      try {
        const msg: Message = JSON.parse(ev.data);
        update((m) => [...m, msg]);
      } catch (_) {}
    });
  }

  function send(user: string, text: string) {
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ user, text }));
    }
  }

  return { subscribe, connect, send, clear: () => set([]) };
}

export const chat = createChatStore();
