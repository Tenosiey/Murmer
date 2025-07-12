import { writable } from 'svelte/store';

export interface Message {
  id?: number;
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
  let oldest = Infinity;

  function on(type: string, cb: (msg: Message) => void) {
    handlers[type] = cb;
  }

  function off(type: string) {
    delete handlers[type];
  }

  function connect(url: string, onOpen?: () => void) {
    if (socket) {
      if (currentUrl === url) return;
      socket.close();
      socket = null;
    }
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
          if (msg.id !== undefined && msg.id < oldest) oldest = msg.id;
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
  }

  async function loadOlder(channel: string) {
    if (!currentUrl) return 0;
    const u = new URL(currentUrl);
    u.protocol = u.protocol.replace('ws', 'http');
    if (u.pathname.endsWith('/ws')) u.pathname = u.pathname.slice(0, -3);
    u.pathname += '/history';
    u.searchParams.set('channel', channel);
    u.searchParams.set('before', String(oldest));
    try {
      const res = await fetch(u.toString());
      if (!res.ok) return 0;
      const msgs: Message[] = await res.json();
      if (msgs.length) {
        msgs.forEach((m) => {
          if (m.id !== undefined && m.id < oldest) oldest = m.id;
        });
        update((cur) => [...msgs.reverse(), ...cur]);
      }
      return msgs.length;
    } catch (e) {
      if (import.meta.env.DEV) console.error('loadOlder failed', e);
      return 0;
    }
  }

  return { subscribe, connect, send, sendRaw, on, off, disconnect, loadOlder, clear: () => { oldest = Infinity; set([]); } };
}

export const chat = createChatStore();
