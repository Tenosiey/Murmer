/**
 * Raised hands in voice channels — the speaking queue for calls with more
 * listeners than talkers.
 *
 * Keyed by account name, with the channel the hand is up in and when it went
 * up (the server's clock, Unix ms), which is the queue order. A user sits in
 * one voice channel at a time, so one entry per user is enough. The server
 * lowers a hand itself when its owner leaves, and the snapshot sent on joining
 * a channel replaces whatever is still held for it from an earlier visit.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message } from '../types';

export interface RaisedHand {
  channelId: number;
  at: number;
}

export type Hands = Record<string, RaisedHand>;

/** Who is waiting to speak in `channelId`, first raised first. */
export function handQueue(hands: Hands, channelId: number): string[] {
  return Object.entries(hands)
    .filter(([, hand]) => hand.channelId === channelId)
    .sort(([a, x], [b, y]) => x.at - y.at || a.localeCompare(b))
    .map(([user]) => user);
}

/** Raise or lower our own hand in the voice channel we are in. */
export function sendHand(channelId: number, raised: boolean) {
  chat.sendRaw({ type: 'voice-hand', channelId, raised });
}

function createVoiceHandsStore() {
  const { subscribe, update } = writable<Hands>({});

  chat.on('voice-hand', (msg: Message) => {
    const { user, channelId, raised, at } = msg;
    if (typeof user !== 'string' || typeof channelId !== 'number') return;
    if (raised === true && typeof at === 'number') {
      update((hands) => ({ ...hands, [user]: { channelId, at } }));
    } else if (raised === false) {
      update((hands) => {
        if (!(user in hands)) return hands;
        const next = { ...hands };
        delete next[user];
        return next;
      });
    }
  });

  chat.on('voice-hands-active', (msg: Message) => {
    const { channelId, hands: snapshot } = msg;
    if (typeof channelId !== 'number' || !snapshot || typeof snapshot !== 'object') return;
    update((hands) => {
      const next: Hands = {};
      for (const [user, hand] of Object.entries(hands)) {
        if (hand.channelId !== channelId) next[user] = hand;
      }
      for (const [user, at] of Object.entries(snapshot)) {
        if (typeof at === 'number') next[user] = { channelId, at };
      }
      return next;
    });
  });

  return { subscribe };
}

export const voiceHands = createVoiceHandsStore();
