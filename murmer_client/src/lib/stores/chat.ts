import { writable, get } from 'svelte/store';
import type { Message } from '../types';
import { session } from './session';
import { notify } from '../notify';

function createChatStore() {
  const { subscribe, update, set } = writable<Message[]>([]);
  let socket: WebSocket | null = null;
  let currentUrl: string | null = null;
  const handlers: Record<string, Array<(msg: Message) => void>> = {};

  function normalizeReactions(value: unknown): Record<string, string[]> {
    if (!value || typeof value !== 'object') return {};
    const result: Record<string, string[]> = {};
    for (const [emoji, users] of Object.entries(value as Record<string, unknown>)) {
      if (!emoji) continue;
      if (Array.isArray(users)) {
        const filtered = users.filter((u): u is string => typeof u === 'string' && u.trim().length > 0);
        result[emoji] = Array.from(new Set(filtered));
      }
    }
    return result;
  }

  function prepareMessage(raw: Message): Message {
    const msg: Message = { ...raw };
    if (typeof msg.timestamp === 'string') {
      const parsed = Date.parse(msg.timestamp);
      if (!Number.isNaN(parsed)) {
        const date = new Date(parsed);
        msg.timestamp = date.toISOString();
        if (!msg.time) {
          msg.time = date.toLocaleTimeString();
        }
      } else {
        msg.timestamp = undefined;
      }
    } else if (msg.timestamp !== undefined) {
      msg.timestamp = undefined;
    }
    if (!msg.time) {
      msg.time = new Date().toLocaleTimeString();
    }
    msg.reactions = normalizeReactions(raw.reactions);
    return msg;
  }

  const REGEX_SPECIALS = new Set(['\\', '.', '+', '*', '?', '^', '$', '{', '}', '(', ')', '|', '[', ']', '/', '-']);

  function escapeRegex(value: string): string {
    let escaped = '';
    for (const char of value) {
      escaped += REGEX_SPECIALS.has(char) ? `\\${char}` : char;
    }
    return escaped;
  }

  function containsMention(text: string | undefined, username: string | null | undefined): boolean {
    if (!text || !username) return false;
    const pattern = new RegExp(`(^|[^\\w@])@${escapeRegex(username)}(?=$|[^\\w-])`, 'i');
    return pattern.test(text);
  }

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
          const prepared = prepareMessage(msg);
          update((m) => [...m, prepared]);
          const current = get(session).user;
          if (!current || prepared.user !== current) {
            const mention = current ? containsMention(prepared.text, current) : false;
            const trimmedText = (prepared.text ?? '').trim();
            if (mention) {
              const body = trimmedText.length > 0 ? trimmedText : `${prepared.user ?? 'Someone'} mentioned you`;
              notify(`Mention from ${prepared.user ?? 'Unknown user'}`, body);
            } else {
              const sender = prepared.user ?? 'Unknown user';
              const body = trimmedText.length > 0 ? trimmedText : 'sent a message';
              notify('New message', `${sender}: ${body}`);
            }
          }
        } else if (msg.type === 'history') {
          const msgs = ((msg.messages as Message[]) || []).map((item) => prepareMessage(item));
          update((m) => [...msgs, ...m]);
          if (handlers['history']) {
            for (const handler of handlers['history']) handler(msg);
          }
        } else if (msg.type === 'reaction-update') {
          const messageId = msg.messageId as number | undefined;
          if (typeof messageId === 'number') {
            const reactions = normalizeReactions(msg.reactions ?? {});
            update((messages) =>
              messages.map((m) => (m.id === messageId ? { ...m, reactions } : m))
            );
          }
          if (handlers['reaction-update']) {
            for (const handler of handlers['reaction-update']) handler(msg);
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
      const now = new Date();
      const payload = {
        type: 'chat',
        user,
        text,
        time: now.toLocaleTimeString(),
        timestamp: now.toISOString()
      };
      if (import.meta.env.DEV) console.log('Sending:', payload);
      socket.send(JSON.stringify(payload));
    }
  }

  function sendRaw(data: any) {
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(data));
    }
  }

  function loadHistory(channel: string, before?: number, limit = 50) {
    sendRaw({ type: 'load-history', channel, before, limit });
  }

  function react(messageId: number, emoji: string, action: 'add' | 'remove') {
    if (!socket || socket.readyState !== WebSocket.OPEN) return;
    if (typeof messageId !== 'number' || Number.isNaN(messageId)) return;
    const trimmed = emoji.trim();
    if (!trimmed) return;
    const payload = { type: 'react', messageId, emoji: trimmed, action };
    if (import.meta.env.DEV) console.log('Sending:', payload);
    socket.send(JSON.stringify(payload));
  }

  function disconnect() {
    if (socket) {
      socket.close();
      // 'close' event handler will reset state
    }
    set([]); // clear chat history on disconnect
  }

  return { subscribe, connect, send, sendRaw, loadHistory, react, on, off, disconnect, clear: () => set([]) };
}

export const chat = createChatStore();
