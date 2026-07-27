/**
 * Voice Activity Detection (VAD) utility
 * Monitors audio levels to detect when the user is speaking
 */
import { getAudioContext, resumeAudioContext } from './audioContext';
import { subscribeTick } from './ticker';

const SMOOTHING_FACTOR = 0.3;
const FFT_SIZE = 256;
const MIN_DECIBELS = -90;
const MAX_DECIBELS = -10;

/**
 * Configure an analyser exactly like the detector does.
 *
 * Shared with the settings level meter (`micLevel.ts`): both must measure the
 * signal identically, otherwise the meter would show a level that cannot be
 * compared against the sensitivity threshold it is drawn next to.
 */
export function configureVadAnalyser(analyser: AnalyserNode): Uint8Array<ArrayBuffer> {
  analyser.fftSize = FFT_SIZE;
  analyser.minDecibels = MIN_DECIBELS;
  analyser.maxDecibels = MAX_DECIBELS;
  analyser.smoothingTimeConstant = SMOOTHING_FACTOR;
  return new Uint8Array(new ArrayBuffer(analyser.frequencyBinCount));
}

/**
 * Read the current input level as the average across the frequency spectrum,
 * normalized to 0-1. This is the value compared against the VAD threshold.
 */
export function readVadLevel(analyser: AnalyserNode, dataArray: Uint8Array<ArrayBuffer>): number {
  analyser.getByteFrequencyData(dataArray);
  let sum = 0;
  for (let i = 0; i < dataArray.length; i++) {
    sum += dataArray[i];
  }
  return sum / dataArray.length / 255;
}

/** How long transmission stays open after the level drops below threshold. */
const HOLD_TIME_MS = 800;

/** Extra delay before closing, so brief dips mid-sentence don't chop words. */
const RELEASE_DELAY_MS = 100;

export class VoiceActivityDetector {
  private analyser: AnalyserNode | null = null;
  /** Node this detector listens on. Owned by the caller, never by us. */
  private source: AudioNode | null = null;
  private dataArray: Uint8Array<ArrayBuffer> | null = null;
  private stopTicks: (() => void) | null = null;
  private isActive = false;
  private currentSensitivity = 0.1;

  // Debouncing for voice activity
  private lastVoiceTime = 0;

  private listeners: Array<(isActive: boolean, level: number) => void> = [];

  /**
   * Start monitoring the given audio node for voice activity.
   *
   * The caller passes a node rather than a stream so the detector sees the
   * signal *after* the input gain — the level compared against the threshold
   * is then the one that is actually transmitted, and the microphone can be
   * turned up without silently making voice detection harder to trigger.
   *
   * The analyser is built on demand: the detector used to open an
   * `AudioContext` at app startup even for users who never enable
   * voice-activity mode, which counted against the browser's context limit
   * for nothing.
   */
  start(source: AudioNode, sensitivity: number = 0.1) {
    // Stop any existing monitoring first
    this.stop();

    const context = getAudioContext();
    if (!context) {
      console.error('Audio context unavailable, cannot detect voice activity');
      return;
    }

    try {
      resumeAudioContext();

      if (!this.analyser) {
        this.analyser = context.createAnalyser();
        this.dataArray = configureVadAnalyser(this.analyser);
      }

      this.currentSensitivity = sensitivity;
      this.source = source;
      this.source.connect(this.analyser);

      this.startAnalysis(sensitivity);
    } catch (error) {
      console.error('Failed to start VAD:', error);
    }
  }

  /**
   * Update sensitivity without restarting the entire VAD
   */
  updateSensitivity(sensitivity: number) {
    this.currentSensitivity = sensitivity;
    // If we're currently monitoring, restart with new sensitivity
    const source = this.source;
    if (source) {
      this.start(source, sensitivity);
    }
  }

  /**
   * Stop monitoring voice activity
   */
  stop() {
    if (this.stopTicks) {
      this.stopTicks();
      this.stopTicks = null;
    }

    if (this.source && this.analyser) {
      // Only the edge into our analyser is cut: the node belongs to the
      // caller's graph and still has to feed the transmission gate.
      try {
        this.source.disconnect(this.analyser);
      } catch {
        // Already disconnected (e.g. the graph was torn down first).
      }
    }
    this.source = null;

    this.isActive = false;
    this.lastVoiceTime = 0;
    this.notifyListeners(false, 0);
  }

  /**
   * Subscribe to voice activity changes
   */
  subscribe(callback: (isActive: boolean, level: number) => void) {
    this.listeners.push(callback);
    return () => {
      this.listeners = this.listeners.filter(cb => cb !== callback);
    };
  }

  /**
   * Sample the level on every audio tick and drive the open/close state
   * machine. Ticks come from the audio thread rather than animation frames
   * because a minimised window stops the latter, which used to freeze the
   * state machine — leaving the microphone open until the window came back.
   */
  private startAnalysis(sensitivity: number) {
    if (!this.analyser || !this.dataArray) return;

    const analyze = () => {
      if (!this.analyser || !this.dataArray) return;

      const normalizedLevel = readVadLevel(this.analyser, this.dataArray);

      // Determine if voice is active based on sensitivity threshold
      // Lower sensitivity values make it more sensitive (easier to trigger)
      const threshold = this.currentSensitivity;
      const currentTime = Date.now();
      const rawVoiceDetected = normalizedLevel > threshold;

      if (rawVoiceDetected) {
        this.lastVoiceTime = currentTime;
        // Immediately activate if not already active
        if (!this.isActive) {
          this.isActive = true;
          this.notifyListeners(true, normalizedLevel);
        }
        return;
      }

      // Below threshold: hold the gate open a moment so a pause mid-sentence
      // doesn't chop the next word, then close it. The deadline is compared
      // against the clock on each tick rather than armed as a `setTimeout`,
      // because timers in a hidden window are throttled to once a second (and
      // eventually once a minute) — long enough to keep the microphone open
      // well after the user stopped talking.
      if (this.isActive && currentTime - this.lastVoiceTime >= HOLD_TIME_MS + RELEASE_DELAY_MS) {
        this.isActive = false;
        this.notifyListeners(false, normalizedLevel);
      }
    };

    this.stopTicks = subscribeTick(analyze);
  }

  private notifyListeners(isActive: boolean, level: number) {
    for (const callback of this.listeners) {
      callback(isActive, level);
    }
  }

  /**
   * Get current audio level (0-1 range)
   */
  getCurrentLevel(): number {
    if (!this.analyser || !this.dataArray) return 0;
    return readVadLevel(this.analyser, this.dataArray);
  }

  /**
   * Check if voice is currently active
   */
  getIsActive(): boolean {
    return this.isActive;
  }

  /**
   * Clean up resources
   */
  destroy() {
    this.stop();
    if (this.analyser) {
      this.analyser.disconnect();
      this.analyser = null;
    }
    this.dataArray = null;
    this.listeners = [];
  }
}