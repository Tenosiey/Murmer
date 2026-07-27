/**
 * The single `AudioContext` shared by every audio graph in the app.
 *
 * Browsers cap the number of concurrent contexts (six in Chromium), and the
 * client used to open one per remote peer plus one each for the microphone
 * graph, the voice-activity detector, the settings level meter and the
 * soundboard. A voice channel with a handful of members exhausted the budget,
 * after which `new AudioContext()` threw and the per-user volume boost
 * silently degraded to the plain `<audio>` fallback.
 *
 * Sharing a context does not merge the graphs: nodes only carry audio where
 * they are explicitly connected. The microphone chain still ends at its own
 * `MediaStreamAudioDestinationNode`, so soundboard clips and notification
 * sounds — which render to `context.destination` — can never leak into the
 * outgoing track.
 *
 * The context lives for the lifetime of the app and is never closed; it is an
 * app-wide resource with no single owner that could close it safely.
 */
import { get } from 'svelte/store';
import { outputDeviceId } from '../stores/settings';

type SinkCapableContext = AudioContext & { setSinkId?: (id: string) => Promise<void> };

let context: SinkCapableContext | null = null;
let creationFailed = false;

/**
 * Route everything rendered to `context.destination` (soundboard clips and
 * the join/leave/mute blips) to the output device picked in settings. Peer
 * voice is routed on the `<audio>` elements instead, because it reaches the
 * speakers through an element rather than through the destination node.
 */
function applySink(deviceId: string | null) {
  if (!context?.setSinkId) return;
  context.setSinkId(deviceId || '').catch((error: unknown) => {
    console.warn('Failed to route app audio to the selected output device:', error);
  });
}

/**
 * The shared context, created on first use. Returns null when the platform
 * refuses to give us one at all — callers fall back to playing audio without
 * a graph rather than going silent.
 */
export function getAudioContext(): AudioContext | null {
  if (context) return context;
  if (creationFailed) return null;
  try {
    context = new AudioContext() as SinkCapableContext;
  } catch (error) {
    creationFailed = true;
    console.error('Failed to create the shared audio context:', error);
    return null;
  }
  applySink(get(outputDeviceId));
  outputDeviceId.subscribe(applySink);
  return context;
}

/**
 * Contexts start suspended until a user gesture, and the OS can suspend them
 * again (e.g. when the output device disappears). Every path that is about to
 * make noise calls this first.
 */
export function resumeAudioContext(): void {
  const ctx = context ?? getAudioContext();
  if (ctx && ctx.state === 'suspended') {
    ctx.resume().catch(() => {
      /* nothing useful to do if the platform refuses */
    });
  }
}
