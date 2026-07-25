/**
 * Live microphone level monitoring for the settings UI.
 *
 * Measures the input exactly like the VAD does (`configureVadAnalyser` /
 * `readVadLevel`) so the reported level can be compared against the sensitivity
 * threshold directly. While a voice session is running it borrows that
 * session's capture stream instead of opening a second capture of the same
 * device; otherwise it opens — and owns — its own stream using the configured
 * input device and microphone processing settings.
 */
import { get } from 'svelte/store';
import {
  inputDeviceId,
  echoCancellation,
  noiseSuppression,
  autoGainControl
} from '../stores/settings';
import { voice } from '../stores/voice';
import { configureVadAnalyser, readVadLevel } from './vad';

export class MicLevelMonitor {
  private audioContext: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private dataArray: Uint8Array<ArrayBuffer> | null = null;
  private animationFrame: number | null = null;
  /** Only set when we opened the stream ourselves and must close it again. */
  private ownedStream: MediaStream | null = null;
  /**
   * Incremented on every start/stop so a `getUserMedia` that resolves after the
   * monitor was stopped (or restarted) tears its stream down instead of
   * leaving the microphone open.
   */
  private generation = 0;

  /**
   * Begin reporting the input level (0-1) on every animation frame.
   * Rejects when the microphone cannot be opened.
   */
  async start(onLevel: (level: number) => void): Promise<void> {
    this.stop();
    const generation = this.generation;

    let stream = voice.captureStream();
    if (!stream) {
      const audio: MediaTrackConstraints = {
        echoCancellation: get(echoCancellation),
        noiseSuppression: get(noiseSuppression),
        autoGainControl: get(autoGainControl)
      };
      const device = get(inputDeviceId);
      if (device) audio.deviceId = { exact: device };
      stream = await navigator.mediaDevices.getUserMedia({ audio });
      if (generation !== this.generation) {
        for (const track of stream.getTracks()) track.stop();
        return;
      }
      this.ownedStream = stream;
    }

    this.audioContext = new AudioContext();
    if (this.audioContext.state === 'suspended') {
      await this.audioContext.resume();
    }
    this.analyser = this.audioContext.createAnalyser();
    this.dataArray = configureVadAnalyser(this.analyser);
    this.source = this.audioContext.createMediaStreamSource(stream);
    this.source.connect(this.analyser);

    const measure = () => {
      if (!this.analyser || !this.dataArray) return;
      onLevel(readVadLevel(this.analyser, this.dataArray));
      this.animationFrame = requestAnimationFrame(measure);
    };
    measure();
  }

  /** Stop measuring and release everything this monitor opened. */
  stop() {
    this.generation++;

    if (this.animationFrame !== null) {
      cancelAnimationFrame(this.animationFrame);
      this.animationFrame = null;
    }
    if (this.source) {
      this.source.disconnect();
      this.source = null;
    }
    this.analyser = null;
    this.dataArray = null;
    if (this.ownedStream) {
      for (const track of this.ownedStream.getTracks()) track.stop();
      this.ownedStream = null;
    }
    if (this.audioContext) {
      const context = this.audioContext;
      this.audioContext = null;
      if (context.state !== 'closed') context.close().catch(() => {});
    }
  }
}
