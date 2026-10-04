import { derived } from 'svelte/store';
import { chat } from './chat';
import { channels } from './channels';

/** channelId -> topic, for the channels that have one. Held by `channels`. */
const topics = derived(channels, ($channels) => {
  const map: Record<number, string> = {};
  for (const ch of $channels) if (ch.topic) map[ch.id] = ch.topic;
  return map;
});

export const channelTopics = {
  subscribe: topics.subscribe,
  /** Send a topic update to the server; the store updates when it broadcasts back. */
  setTopic(channelId: number, topic: string) {
    if (!channelId) return;
    chat.sendRaw({ type: 'set-channel-topic', channelId, topic: topic.trim() });
  }
};
