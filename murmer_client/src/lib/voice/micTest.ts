/**
 * Microphone test / loopback for the settings UI.
 *
 * Records a few seconds of the input exactly as a voice session would capture
 * it — same device, same processing constraints, same noise suppression, same
 * input gain — and plays it back locally. A level bar only says "something is
 * arriving"; hearing the recording is the only way to judge what echo
 * cancellation, noise suppression and automatic gain control are doing to a
 * voice, and it is how the two noise suppression modes get compared.
 *
 * Playback is rendered to the shared context's destination, which is routed to
 * the selected output device. That destination is *not* the microphone chain's
 * own `MediaStreamAudioDestinationNode`, so a test played back during a call
 * cannot leak into the outgoing track — nobody else hears it.
 */
import { get } from 'svelte/store';
import { micGain, clampMicGain } from '../stores/settings';
import { captureStream } from '../stores/voiceCapture';
import { refreshAudioDevices } from '../stores/audioDevices';
import { openMicrophone } from './capture';
import { connectMicSource, type MicSource } from './denoise';
import { getAudioContext, resumeAudioContext } from './audioContext';

const WORKLET_URL = '/audio/recorder-processor.js';

/** How long a test recording runs before it stops on its own. */
export const MIC_TEST_SECONDS = 6;

export class MicTest {
  private context: AudioContext | null = null;
  /** Only set when we opened the stream ourselves and must close it again. */
  private ownedStream: MediaStream | null = null;
  private source: MicSource | null = null;
  private gain: GainNode | null = null;
  private recorder: AudioWorkletNode | null = null;
  /** Muted sink that keeps the graph pulling the recorder node. */
  private sink: GainNode | null = null;
  private chunks: Float32Array[] = [];
  private frames = 0;
  private finish: ((buffer: AudioBuffer | null) => void) | null = null;
  private playback: AudioBufferSourceNode | null = null;
  /**
   * Incremented on every start/stop so a `getUserMedia` or module load that
   * resolves after the test was abandoned tears its stream down instead of
   * leaving the microphone open.
   */
  private generation = 0;

  /**
   * Record up to `MIC_TEST_SECONDS` of microphone input.
   *
   * Resolves with the recording, or with null when it was abandoned before
   * anything was captured. Rejects when the microphone or the audio graph is
   * unavailable — the caller shows that as a message rather than a silent
   * no-op.
   */
  async record(onProgress: (seconds: number) => void): Promise<AudioBuffer | null> {
    // Settles any pending recording and stops a playback that is still
    // running — recording while the previous test plays back would capture it.
    this.dispose();
    const generation = this.generation;

    const context = getAudioContext();
    if (!context?.audioWorklet) {
      throw new Error('Audio recording is unavailable on this system');
    }
    resumeAudioContext();
    // Loading the same module twice is a no-op, so this needs no caching.
    await context.audioWorklet.addModule(WORKLET_URL);

    // A running session's capture is borrowed rather than opening a second
    // capture of the same device.
    let stream = get(captureStream);
    if (!stream) {
      stream = await openMicrophone();
      if (generation !== this.generation) {
        for (const track of stream.getTracks()) track.stop();
        return null;
      }
      this.ownedStream = stream;
      // First capture of the session: device labels only become readable once
      // permission has been granted, so re-enumerate for the settings selects.
      void refreshAudioDevices();
    } else if (generation !== this.generation) {
      return null;
    }

    // Built into locals and only published once the chain is complete: loading
    // RNNoise takes a moment, and a test abandoned in the middle of it must
    // take its whole graph with it rather than leave half of it connected.
    // The same amplification the outgoing chain applies, so the playback is
    // as loud as the peers hear it.
    const gain = context.createGain();
    gain.gain.value = clampMicGain(get(micGain));
    const recorder = new AudioWorkletNode(context, 'recorder-processor', {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      outputChannelCount: [1]
    });
    // The recorder writes no output; it is connected only so the rendering
    // graph keeps calling its `process`.
    const sink = context.createGain();
    sink.gain.value = 0;
    gain.connect(recorder);
    recorder.connect(sink);
    sink.connect(context.destination);
    // Same noise suppression the outgoing chain runs, which is most of what
    // this test exists to let the user judge.
    const source = await connectMicSource(context, stream, gain);
    if (generation !== this.generation) {
      source.disconnect();
      gain.disconnect();
      recorder.disconnect();
      sink.disconnect();
      return null;
    }

    this.context = context;
    this.gain = gain;
    this.recorder = recorder;
    this.sink = sink;
    this.source = source;

    const limit = Math.round(MIC_TEST_SECONDS * context.sampleRate);
    return new Promise<AudioBuffer | null>((resolve) => {
      this.finish = resolve;
      this.recorder!.port.onmessage = (event: MessageEvent<Float32Array>) => {
        if (generation !== this.generation) return;
        this.chunks.push(event.data);
        this.frames += event.data.length;
        onProgress(this.frames / context.sampleRate);
        if (this.frames >= limit) this.stopRecording();
      };
    });
  }

  /**
   * End the recording early and keep what was captured, so stopping a test
   * mid-sentence still plays back the part that was recorded.
   */
  stopRecording() {
    const finish = this.finish;
    if (!finish) return;
    const buffer = this.buildBuffer();
    this.release();
    finish(buffer);
  }

  /** Play a recording back through the selected output device. */
  play(buffer: AudioBuffer, onEnded: () => void) {
    const context = getAudioContext();
    if (!context) return;
    resumeAudioContext();
    this.stopPlayback();

    const source = context.createBufferSource();
    source.buffer = buffer;
    source.connect(context.destination);
    source.onended = () => {
      // A manual stop also fires this; the caller has already reset its state
      // in that case, so only the natural end reports back.
      if (this.playback !== source) return;
      this.playback = null;
      onEnded();
    };
    this.playback = source;
    source.start();
  }

  stopPlayback() {
    if (!this.playback) return;
    const source = this.playback;
    this.playback = null;
    try {
      source.stop();
    } catch {
      // Already finished.
    }
    source.disconnect();
  }

  /** Abandon anything in flight and release the microphone. */
  dispose() {
    this.stopPlayback();
    const finish = this.finish;
    this.release();
    finish?.(null);
  }

  private buildBuffer(): AudioBuffer | null {
    const context = this.context;
    if (!context || this.frames === 0) return null;
    const buffer = context.createBuffer(1, this.frames, context.sampleRate);
    const channel = buffer.getChannelData(0);
    let offset = 0;
    for (const chunk of this.chunks) {
      channel.set(chunk, offset);
      offset += chunk.length;
    }
    return buffer;
  }

  /** Tear down the recording graph. Playback is deliberately left alone. */
  private release() {
    this.generation++;
    this.finish = null;
    this.chunks = [];
    this.frames = 0;

    if (this.recorder) {
      this.recorder.port.onmessage = null;
      this.recorder.disconnect();
      this.recorder = null;
    }
    if (this.source) {
      this.source.disconnect();
      this.source = null;
    }
    if (this.gain) {
      this.gain.disconnect();
      this.gain = null;
    }
    if (this.sink) {
      this.sink.disconnect();
      this.sink = null;
    }
    if (this.ownedStream) {
      for (const track of this.ownedStream.getTracks()) track.stop();
      this.ownedStream = null;
    }
    this.context = null;
  }
}
