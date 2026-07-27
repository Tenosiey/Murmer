/**
 * Soundboard playback.
 *
 * Sounds are *not* mixed into anyone's microphone: the server only relays a
 * `soundboard-play` frame and every client in the voice channel fetches and
 * plays the file itself. That is what makes per-listener volume and per-sound
 * mute possible, and it keeps the clip out of the 64 kbit mono voice codec.
 *
 * Playback renders to the shared context's destination — never into the
 * microphone chain, which ends at its own `MediaStreamAudioDestinationNode`,
 * so a sound cannot leak into the outgoing track. Decoded buffers are cached
 * so a repeated sound costs one gain node.
 */
import { get } from 'svelte/store';
import { chat } from '../stores/chat';
import { selectedServer } from '../stores/servers';
import { outputMuted } from '../stores/settings';
import { effectiveGain } from '../stores/soundboardSettings';
import { httpBaseFromWs } from '../server-url';
import { getAudioContext, resumeAudioContext } from './audioContext';
import type { Message } from '../types';

/** A playback that reached the speakers, for the transient "played X" hint. */
export interface SoundPlayEvent {
  soundId: number;
  name: string;
  user: string;
  channelId: number;
}

export class SoundboardPlayer {
  private buffers = new Map<number, AudioBuffer>();
  /** In-flight decodes, so a burst of the same sound fetches it once. */
  private pending = new Map<number, Promise<AudioBuffer | null>>();
  private channelId: number | null = null;
  private listeners: Array<(event: SoundPlayEvent) => void> = [];

  // Kept so `destroy` can unregister exactly these handlers: `chat.off(type)`
  // without a callback drops every handler for the type, which would take the
  // soundboard store's own `sound-list` subscription with it.
  private readonly onPlay = (msg: Message) => {
    void this.handlePlay(msg);
  };
  private readonly onSoundList = () => {
    // A sound may have been replaced under the same id; drop the cache rather
    // than risk playing a stale clip.
    this.buffers.clear();
    this.pending.clear();
  };

  constructor() {
    chat.on('soundboard-play', this.onPlay);
    chat.on('sound-list', this.onSoundList);
  }

  /** Notify subscribers about sounds that actually played locally. */
  subscribe(cb: (event: SoundPlayEvent) => void) {
    this.listeners.push(cb);
    return () => {
      this.listeners = this.listeners.filter((fn) => fn !== cb);
    };
  }

  /** Track which voice channel we are in; frames for others are ignored. */
  setChannel(channelId: number | null) {
    this.channelId = channelId;
    if (channelId === null) {
      this.buffers.clear();
      this.pending.clear();
    }
  }


  /** Fetch and decode a sound, caching the result. */
  private async loadBuffer(context: AudioContext, id: number, url: string) {
    const cached = this.buffers.get(id);
    if (cached) return cached;
    const inFlight = this.pending.get(id);
    if (inFlight) return inFlight;

    const server = get(selectedServer);
    if (!server) return null;

    const request = (async () => {
      try {
        const response = await fetch(`${httpBaseFromWs(server)}${url}`);
        if (!response.ok) return null;
        const bytes = await response.arrayBuffer();
        const buffer = await context.decodeAudioData(bytes);
        this.buffers.set(id, buffer);
        return buffer;
      } catch (error) {
        console.error('Failed to load soundboard sound:', error);
        return null;
      } finally {
        this.pending.delete(id);
      }
    })();
    this.pending.set(id, request);
    return request;
  }

  private async handlePlay(msg: Message) {
    const soundId = (msg as any).id;
    const channelId = (msg as any).channelId;
    const user = (msg as any).user;
    const name = (msg as any).name;
    const url = (msg as any).url;

    // Validate the frame before acting on it; in particular a non-relative URL
    // must never be fetched.
    if (typeof soundId !== 'number' || !Number.isFinite(soundId)) return;
    if (typeof channelId !== 'number' || channelId !== this.channelId) return;
    if (typeof user !== 'string' || typeof name !== 'string') return;
    if (typeof url !== 'string' || !url.startsWith('/files/')) return;

    // Deafening means hearing nothing from the channel — clips included.
    if (get(outputMuted)) return;

    const gain = effectiveGain(soundId, user);
    if (gain === null) return;

    const context = getAudioContext();
    if (!context) return;
    // Browsers start the context suspended until a user gesture; by the time a
    // sound arrives the user has clicked their way into a voice channel.
    resumeAudioContext();

    const buffer = await this.loadBuffer(context, soundId, url);
    if (!buffer) return;
    // The channel may have changed while the file was downloading.
    if (channelId !== this.channelId) return;

    const source = context.createBufferSource();
    source.buffer = buffer;
    const gainNode = context.createGain();
    gainNode.gain.value = gain;
    source.connect(gainNode);
    gainNode.connect(context.destination);
    source.onended = () => {
      source.disconnect();
      gainNode.disconnect();
    };
    source.start();

    for (const cb of this.listeners) cb({ soundId, name, user, channelId });
  }

  destroy() {
    chat.off('soundboard-play', this.onPlay);
    chat.off('sound-list', this.onSoundList);
    this.buffers.clear();
    this.pending.clear();
  }
}
