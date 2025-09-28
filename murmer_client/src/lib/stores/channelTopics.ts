import { writable } from 'svelte/store';

function createChannelTopicStore() {
  const { subscribe, update } = writable<Record<string, string>>({});

  function setTopic(channel: string, topic: string) {
    if (!channel) return;
    const trimmed = topic.trim();
    update((topics) => {
      const next = { ...topics };
      if (trimmed) {
        next[channel] = trimmed;
      } else {
        delete next[channel];
      }
      return next;
    });
  }

  return { subscribe, setTopic };
}

export const channelTopics = createChannelTopicStore();
