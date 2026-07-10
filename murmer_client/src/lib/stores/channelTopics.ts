import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

function createChannelTopicStore() {
  const { subscribe, update, set } = writable<Record<number, string>>({});

  function applyTopic(channelId: number, topic: string) {
    const trimmed = topic.trim();
    update((topics) => {
      const next = { ...topics };
      if (trimmed) {
        next[channelId] = trimmed;
      } else {
        delete next[channelId];
      }
      return next;
    });
  }

  chat.on('channel-list', (msg: Message) => {
    const list = (msg as any).channels;
    if (!Array.isArray(list)) return;
    const topics: Record<number, string> = {};
    for (const item of list) {
      if (
        item &&
        typeof item === 'object' &&
        typeof item.id === 'number' &&
        typeof item.topic === 'string' &&
        item.topic.trim()
      ) {
        topics[item.id] = item.topic.trim();
      }
    }
    set(topics);
  });

  chat.on('channel-topic', (msg: Message) => {
    const raw = msg as any;
    if (typeof raw.channelId !== 'number' || typeof raw.topic !== 'string') return;
    applyTopic(raw.channelId, raw.topic);
  });

  chat.on('channel-remove', (msg: Message) => {
    const id = (msg as any).channelId;
    if (typeof id === 'number') {
      update((topics) => {
        if (!(id in topics)) return topics;
        const next = { ...topics };
        delete next[id];
        return next;
      });
    }
  });

  /** Send a topic update to the server; the store updates when it broadcasts back. */
  function setTopic(channelId: number, topic: string) {
    if (!channelId) return;
    chat.sendRaw({ type: 'set-channel-topic', channelId, topic: topic.trim() });
  }

  return { subscribe, setTopic };
}

export const channelTopics = createChannelTopicStore();
