/**
 * One shared ~60 Hz tick that drives every level measurement in the app: the
 * voice-activity detector, the settings input meter, our own speaking
 * indicator and the one on each remote peer.
 *
 * It is backed by an audio worklet rather than `requestAnimationFrame`,
 * because rAF stops completely while the window is minimised or hidden. That
 * froze the VAD state machine mid-flight — if it happened to be transmitting,
 * the microphone stayed open until the window came back. The audio thread
 * keeps rendering no matter what the window is doing.
 *
 * `requestAnimationFrame` remains the fallback for the case where the worklet
 * cannot be loaded at all, which is no worse than what came before.
 */
import { getAudioContext, resumeAudioContext } from './audioContext';

const TICK_INTERVAL_MS = 16;
const WORKLET_URL = '/audio/tick-processor.js';

type Tick = () => void;

const listeners = new Set<Tick>();

let workletNode: AudioWorkletNode | null = null;
/** Muted sink that keeps the worklet node pulled by the graph. */
let workletSink: GainNode | null = null;
let workletLoading: Promise<boolean> | null = null;
let rafHandle: number | null = null;

function emit() {
  // Iterating the set directly is safe: removing an entry mid-iteration is
  // well defined, and this runs ~60 times a second, so the copy a snapshot
  // would need is worth avoiding.
  for (const listener of listeners) {
    try {
      listener();
    } catch (error) {
      console.error('Audio tick listener failed:', error);
    }
  }
}

function startRaf() {
  if (rafHandle !== null) return;
  const step = () => {
    rafHandle = requestAnimationFrame(step);
    emit();
  };
  rafHandle = requestAnimationFrame(step);
}

function stopRaf() {
  if (rafHandle === null) return;
  cancelAnimationFrame(rafHandle);
  rafHandle = null;
}

/**
 * Build the worklet node once and keep it for the lifetime of the app.
 * Resolves false when the platform has no usable `audioWorklet`, in which
 * case the ticker stays on rAF for good.
 */
function ensureWorklet(): Promise<boolean> {
  if (workletLoading) return workletLoading;
  workletLoading = (async () => {
    const context = getAudioContext();
    if (!context?.audioWorklet) return false;
    try {
      await context.audioWorklet.addModule(WORKLET_URL);
      workletNode = new AudioWorkletNode(context, 'tick-processor', {
        numberOfInputs: 0,
        numberOfOutputs: 1,
        outputChannelCount: [1],
        processorOptions: { intervalMs: TICK_INTERVAL_MS }
      });
      // The node produces no sound; it is connected only so the rendering
      // graph keeps calling its `process`.
      workletSink = context.createGain();
      workletSink.gain.value = 0;
      workletNode.connect(workletSink);
      workletSink.connect(context.destination);
      workletNode.port.onmessage = () => {
        if (listeners.size > 0) emit();
      };
      return true;
    } catch (error) {
      console.warn('Audio tick worklet unavailable, falling back to animation frames:', error);
      workletNode = null;
      workletSink = null;
      return false;
    }
  })();
  return workletLoading;
}

/**
 * Run `callback` on every tick until the returned function is called.
 *
 * The first subscriber starts on animation frames and switches over as soon
 * as the worklet is ready, so a meter that mounts and unmounts quickly never
 * waits on module loading. With nobody listening the worklet is told to go
 * idle so it stops posting messages nothing would act on.
 */
export function subscribeTick(callback: Tick): () => void {
  listeners.add(callback);
  if (listeners.size === 1) {
    if (workletNode) {
      workletNode.port.postMessage({ active: true });
      resumeAudioContext();
    } else {
      startRaf();
      void ensureWorklet().then((ready) => {
        if (!ready) return;
        // Only hand over once the worklet is actually ticking, and only while
        // somebody is still listening.
        if (listeners.size > 0) {
          resumeAudioContext();
          stopRaf();
        } else {
          workletNode?.port.postMessage({ active: false });
        }
      });
    }
  }
  return () => {
    listeners.delete(callback);
    if (listeners.size === 0) {
      stopRaf();
      workletNode?.port.postMessage({ active: false });
    }
  };
}
