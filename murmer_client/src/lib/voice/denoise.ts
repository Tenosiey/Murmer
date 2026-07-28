/**
 * RNNoise noise suppression, and the shared front of every microphone chain.
 *
 * RNNoise is a small recurrent network trained on speech plus noise. Unlike the
 * browser's `noiseSuppression` capture constraint — a stationary-noise filter
 * that only really removes steady hum — it removes keyboards, fans, chairs and
 * babble while leaving the voice intact. It runs locally in an audio worklet at
 * roughly a percent of one core.
 *
 * Where it sits: *before* the input gain, so it sees speech at capture level
 * rather than whatever the "Input volume" slider has multiplied it by, and so
 * everything downstream — the transmission gate, the voice detector and both
 * settings meters — measures the denoised signal. That keeps the invariant the
 * rest of the voice code is built on: the level compared against the VAD
 * threshold is the one the peers actually receive. It also makes voice
 * detection much steadier, because the noise floor it tracks collapses towards
 * zero once the room noise is gone.
 *
 * Three constraints shape the loading below:
 *
 * - An `AudioWorkletGlobalScope` has no `fetch`, so the WASM binary is fetched
 *   here on the main thread and handed to the processor as an `ArrayBuffer`.
 * - Compiling WASM needs `'wasm-unsafe-eval'` in the CSP. `tauri.conf.json`
 *   grants it; note that the dev server sends no CSP at all, so a mistake there
 *   only shows up in a packaged build.
 * - RNNoise is trained on 48 kHz and its worklet rejects anything else, hence
 *   the rate the shared context asks for and the check below.
 *
 * Every failure here is non-fatal: the chain is built without the node and the
 * user keeps a working microphone.
 */
import { get } from 'svelte/store';
import { RnnoiseWorkletNode, loadRnnoise } from '@sapphi-red/web-noise-suppressor';
import rnnoiseWorkletUrl from '@sapphi-red/web-noise-suppressor/rnnoiseWorklet.js?url';
import rnnoiseWasmUrl from '@sapphi-red/web-noise-suppressor/rnnoise.wasm?url';
import rnnoiseSimdWasmUrl from '@sapphi-red/web-noise-suppressor/rnnoise_simd.wasm?url';
import { noiseSuppressionMode } from '../stores/settings';

/** The only rate RNNoise is trained for. */
const REQUIRED_SAMPLE_RATE = 48_000;

/**
 * Channels a single node will denoise. One per channel costs its own network
 * state, which is why this is a bound rather than "all of them" — but it has to
 * be more than one: a stereo capture whose second channel is not processed
 * comes out with that channel silent, not merely unfiltered.
 */
const MAX_CHANNELS = 2;

/** Resolved WASM binary, kept for the lifetime of the app. */
let wasmBinary: ArrayBuffer | null = null;
let loading: Promise<boolean> | null = null;

/**
 * Fetch the WASM binary and register the worklet, once.
 *
 * Resolves false when RNNoise cannot run here — no worklet support, a context
 * that is not at 48 kHz, a blocked or failed load. The result is memoised
 * either way: a platform that cannot do this will not start being able to.
 */
export function ensureRnnoise(context: AudioContext): Promise<boolean> {
  if (loading) return loading;
  loading = (async () => {
    if (!context.audioWorklet) {
      console.warn('RNNoise unavailable: this platform has no audio worklet support');
      return false;
    }
    if (context.sampleRate !== REQUIRED_SAMPLE_RATE) {
      console.warn(
        `RNNoise unavailable: it needs ${REQUIRED_SAMPLE_RATE} Hz, the audio context runs at ${context.sampleRate} Hz`
      );
      return false;
    }
    try {
      // The SIMD build is picked automatically where the platform supports it.
      wasmBinary = await loadRnnoise({ url: rnnoiseWasmUrl, simdUrl: rnnoiseSimdWasmUrl });
      await context.audioWorklet.addModule(rnnoiseWorkletUrl);
      return true;
    } catch (error) {
      console.warn('Failed to load RNNoise, falling back to the capture constraint:', error);
      wasmBinary = null;
      return false;
    }
  })();
  return loading;
}

/**
 * A microphone hooked into a graph, and the way to take it back out.
 *
 * Callers hold this instead of the individual nodes: whether there is a
 * denoiser in the middle is this module's business, and tearing the chain down
 * has to release the RNNoise state as well as disconnect the nodes.
 */
export interface MicSource {
  /** Disconnect every node this built and free the RNNoise state. */
  disconnect(): void;
}

/**
 * Connect `stream` to `target`, denoising on the way when the user asked for
 * it and the platform can deliver it.
 *
 * Shared by the voice manager, the settings level meter and the microphone
 * test, so all three measure and play back the same signal — a meter that
 * showed the undenoised level could not be compared against the threshold it is
 * drawn next to, and a test recording that skipped the filter would not be a
 * test of what the peers hear.
 *
 * Failing to build the denoiser connects the microphone directly rather than
 * throwing: no noise suppression is a much smaller problem than no microphone.
 */
export async function connectMicSource(
  context: AudioContext,
  stream: MediaStream,
  target: AudioNode
): Promise<MicSource> {
  const denoiser = get(noiseSuppressionMode) === 'rnnoise' ? await createDenoiser(context) : null;
  const source = context.createMediaStreamSource(stream);

  if (denoiser) {
    source.connect(denoiser);
    denoiser.connect(target);
  } else {
    source.connect(target);
  }

  return {
    disconnect() {
      source.disconnect();
      if (denoiser) {
        denoiser.disconnect();
        // Frees the per-channel network state inside the worklet; without it
        // every device swap would leak one set of them for the session.
        denoiser.destroy();
      }
    }
  };
}

/**
 * Build one RNNoise node, or null when it cannot run.
 *
 * The node passes silence for the few milliseconds its processor spends
 * instantiating the WASM module. That is invisible in practice: at join time
 * the transmission gate is still closed, and a device swap mid-call already
 * costs longer than that in `getUserMedia`.
 */
async function createDenoiser(context: AudioContext): Promise<RnnoiseWorkletNode | null> {
  if (!(await ensureRnnoise(context)) || !wasmBinary) return null;
  try {
    return new RnnoiseWorkletNode(context, { maxChannels: MAX_CHANNELS, wasmBinary });
  } catch (error) {
    console.warn('Failed to create the RNNoise node, continuing without it:', error);
    return null;
  }
}
