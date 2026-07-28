/**
 * Live microphone level monitoring for the settings UI.
 *
 * Measures the input exactly like the VAD does (`configureVadAnalyser` /
 * `readVadLevel`) so the reported level can be compared against the sensitivity
 * threshold directly — which includes applying the same input gain the voice
 * manager puts ahead of its detector. While a voice session is running it
 * borrows that session's capture stream instead of opening a second capture of
 * the same device; otherwise it opens — and owns — its own stream using the
 * configured input device and microphone processing settings.
 */
import { get } from 'svelte/store';
import { micGain, clampMicGain } from '../stores/settings';
import { captureStream } from '../stores/voiceCapture';
import { refreshAudioDevices } from '../stores/audioDevices';
import { openMicrophone } from './capture';
import { getAudioContext, resumeAudioContext } from './audioContext';
import { subscribeTick } from './ticker';
import { configureVadAnalyser, NoiseFloorTracker, readVadLevel } from './vad';

export class MicLevelMonitor {
  private analyser: AnalyserNode | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private gain: GainNode | null = null;
  private stopGainSubscription: (() => void) | null = null;
  private dataArray: Uint8Array<ArrayBuffer> | null = null;
  private stopTicks: (() => void) | null = null;
  /**
   * Own estimate of the background noise, so the meter can draw the threshold
   * automatic mode would use. The detector tracks its own on the same signal —
   * sharing one would mean the marker went blank whenever the other side is
   * not running, which is most of the time the settings are open.
   */
  private noiseFloor = new NoiseFloorTracker();
  /** Only set when we opened the stream ourselves and must close it again. */
  private ownedStream: MediaStream | null = null;
  /**
   * Incremented on every start/stop so a `getUserMedia` that resolves after the
   * monitor was stopped (or restarted) tears its stream down instead of
   * leaving the microphone open.
   */
  private generation = 0;

  /**
   * Begin reporting the input level and the threshold automatic sensitivity
   * derives from the current noise floor (both 0-1) on every audio tick.
   * Rejects when the microphone cannot be opened.
   */
  async start(onSample: (level: number, autoThreshold: number) => void): Promise<void> {
    this.stop();
    const generation = this.generation;

    let stream = get(captureStream);
    if (!stream) {
      stream = await openMicrophone();
      if (generation !== this.generation) {
        for (const track of stream.getTracks()) track.stop();
        return;
      }
      this.ownedStream = stream;
      // First capture of the session: device labels only become readable once
      // permission has been granted, so re-enumerate for the settings selects.
      void refreshAudioDevices();
    }

    const context = getAudioContext();
    if (!context) throw new Error('Audio processing is unavailable on this system');
    resumeAudioContext();

    this.analyser = context.createAnalyser();
    this.dataArray = configureVadAnalyser(this.analyser);
    this.source = context.createMediaStreamSource(stream);
    // The borrowed capture stream is the *raw* microphone, so the gain has to
    // be re-applied here; dragging the slider updates it without a restart.
    this.gain = context.createGain();
    this.stopGainSubscription = micGain.subscribe((value) => {
      if (this.gain) this.gain.gain.value = clampMicGain(value);
    });
    this.source.connect(this.gain);
    this.gain.connect(this.analyser);

    this.noiseFloor.reset();
    this.stopTicks = subscribeTick(() => {
      if (!this.analyser || !this.dataArray) return;
      const level = readVadLevel(this.analyser, this.dataArray);
      onSample(level, this.noiseFloor.update(level));
    });
  }

  /** Stop measuring and release everything this monitor opened. */
  stop() {
    this.generation++;

    if (this.stopTicks) {
      this.stopTicks();
      this.stopTicks = null;
    }
    if (this.stopGainSubscription) {
      this.stopGainSubscription();
      this.stopGainSubscription = null;
    }
    if (this.source) {
      this.source.disconnect();
      this.source = null;
    }
    if (this.gain) {
      this.gain.disconnect();
      this.gain = null;
    }
    if (this.analyser) {
      this.analyser.disconnect();
      this.analyser = null;
    }
    this.dataArray = null;
    if (this.ownedStream) {
      for (const track of this.ownedStream.getTracks()) track.stop();
      this.ownedStream = null;
    }
  }
}
