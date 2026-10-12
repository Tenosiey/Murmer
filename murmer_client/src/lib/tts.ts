/**
 * Text-to-speech for `/tts` messages, through the platform's own
 * `speechSynthesis` — no voice model ships with Murmer.
 *
 * Only live messages are spoken, never history: scrolling back must not
 * replay a conversation. The flag is the sender's request; `ttsEnabled` is
 * the listener's answer, and a muted channel stays silent either way.
 */
import { get } from 'svelte/store';
import { chat } from '#lib/stores/chat.ts';
import { channelNotifications } from '#lib/stores/channelNotifications.ts';
import { isBlocked } from '#lib/stores/blocks.ts';
import { displayNames } from '#lib/stores/profiles.ts';
import { ttsEnabled } from '#lib/stores/settings.ts';
import { hideSpoilers } from '#lib/spoilers.ts';
import type { Message } from '#lib/types.ts';

/** What to say for a message: who, then what, with spoilers kept hidden. */
export function ttsUtterance(name: string, text: string): string {
  return `${name} says ${hideSpoilers(text).trim()}`;
}

chat.onLiveMessage((msg: Message) => {
  if (msg.tts !== true || typeof msg.text !== 'string' || !msg.text.trim()) return;
  if (!msg.user || typeof msg.channelId !== 'number' || isBlocked(msg.user)) return;
  if (!get(ttsEnabled) || typeof speechSynthesis === 'undefined') return;
  if (get(channelNotifications)[msg.channelId] === 'mute') return;
  speechSynthesis.speak(
    new SpeechSynthesisUtterance(ttsUtterance(get(displayNames)(msg.user), msg.text))
  );
});
