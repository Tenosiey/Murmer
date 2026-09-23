// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { outputMuted, userVolumes, volume } from '../stores/settings';
import { remoteAudio } from './remoteAudio';

/**
 * A peer at the wrong volume, or one that stays audible through the output
 * mute, is noticed by the other people in the call rather than by whoever
 * broke it. This covers the path without an audio graph, where the element
 * carries every factor and a per-user boost has to be clamped to 100%.
 */
vi.mock('./audioContext', () => ({ getAudioContext: () => null, resumeAudioContext: () => {} }));

const peer = () => ({ stream: {} as MediaStream, userId: 'bob' });

beforeEach(() => {
  volume.set(1);
  outputMuted.set(false);
  userVolumes.set({});
});

describe('remoteAudio without an audio graph', () => {
  it('multiplies the global and the per-user volume', () => {
    volume.set(0.5);
    userVolumes.set({ bob: 0.4 });
    const node = document.createElement('audio');
    const action = remoteAudio(node, peer());
    expect(node.volume).toBeCloseTo(0.2);
    action.destroy();
  });

  it('follows later changes and clamps a boost to 100%', () => {
    const node = document.createElement('audio');
    const action = remoteAudio(node, peer());
    volume.set(0.5);
    userVolumes.set({ bob: 2 });
    expect(node.volume).toBeCloseTo(0.5);
    outputMuted.set(true);
    expect(node.volume).toBe(0);
    action.destroy();
  });

  it('stops following the stores once destroyed', () => {
    const node = document.createElement('audio');
    remoteAudio(node, peer()).destroy();
    volume.set(0.3);
    expect(node.volume).toBe(1);
  });
});
