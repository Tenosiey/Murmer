import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export interface Message {
  type: string;
  user: string;
  text?: string;
  time?: string;
  [key: string]: unknown;
}

function createChatStore() {
  const { subscribe, update, set } = writable<Message[]>([]);
  let socket: WebSocket | null = null;
  let currentUrl: string | null = null;
  const handlers: Record<string, (msg: Message) => void> = {};

  function on(type: string, cb: (msg: Message) => void) {
    handlers[type] = cb;
  }

  function off(type: string) {
    delete handlers[type];
  }

  let currentUser: string | null = null;
  let currentPassword: string | undefined;

  function connect(url: string, user: string | null, password: string | undefined, onOpen?: () => void) {
    currentUser = user;
    currentPassword = password;
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
    socket.addEventListener('message', async (ev) => {
      if (import.meta.env.DEV) console.log('Received:', ev.data);
      try {
        const msg: Message = JSON.parse(ev.data);
        if (msg.type === 'chat') {
          if (!msg.time) msg.time = new Date().toLocaleTimeString();
          update((m) => [...m, msg]);
        } else if (msg.type === 'challenge') {
          const nonce = msg.nonce as string;
          try {
            const publicKey = await invoke<string>('get_public_key');
            const signature = await invoke<string>('sign_data', { data: nonce });
            socket?.send(
              JSON.stringify({
                type: 'presence',
                user: currentUser,
                nonce,
                publicKey,
                signature,
                password: currentPassword
              })
            );
          } catch (e) {
            console.error('signing failed', e);
          }
        } else if (msg.type && handlers[msg.type]) {
          handlers[msg.type](msg);
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
