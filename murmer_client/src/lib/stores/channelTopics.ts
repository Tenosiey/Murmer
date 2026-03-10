import { writable } from 'svelte/store';

function createChannelTopicStore() {
  const { subscribe, update } = writable<Record<number, string>>({});

  function setTopic(channelId: number, topic: string) {
    if (!channelId) return;
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

  return { subscribe, setTopic };
}

export const channelTopics = createChannelTopicStore();
