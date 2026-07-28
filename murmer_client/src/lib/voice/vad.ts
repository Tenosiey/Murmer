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

/**
 * Ends of the threshold scale, shared by the manual slider, the level meter
 * drawn under it and the clamp applied to automatically derived thresholds.
 */
export const VAD_THRESHOLD_MIN = 0.01;
export const VAD_THRESHOLD_MAX = 0.5;

/**
 * Turn a tracked noise floor into the level speech has to exceed. The margin
 * is proportional (a noisy room needs a bigger gap in absolute terms) plus a
 * fixed offset so a near-silent input still gets a floor worth clearing.
 */
const NOISE_FLOOR_MARGIN = 1.6;
const NOISE_FLOOR_OFFSET = 0.03;

/** How fast the floor follows the input downwards, as a fraction per second. */
const FLOOR_DECAY_PER_SECOND = 3;

/** How fast the floor may rise, in level units per second. Deliberately far
 *  slower than it falls: a floor that is too low passes some noise, one that
 *  overshoots cuts the speaker off. */
const FLOOR_CREEP_PER_SECOND = 0.004;

/**
 * How long the input has to stay quiet before the floor may rise again.
 *
 * This is what keeps long sentences intact. The gaps between words fall below
 * the threshold but stay well above the floor, so without a dwell the floor
 * crept up a little on every gap; over a long sentence that added up until the
 * threshold overtook the speaker and cut them off mid-word. Anything shorter
 * than a real pause between sentences must not count as "the room is quiet
 * now", so this is comfortably longer than the gate's own `HOLD_TIME_MS`.
 */
const QUIET_DWELL_MS = 2500;

/**
 * Window the floor may never rise above the quietest level of. Tracked as two
 * halves so it costs two numbers instead of a buffer of every sample.
 *
 * This is the hard guarantee that speech cannot ratchet the threshold up:
 * however long somebody talks, they breathe, and the level between syllables
 * drops back to the room. The floor can never climb past that, so it can never
 * climb past the speaker.
 */
const FLOOR_WINDOW_MS = 20_000;

/**
 * How far above the floor the window's quietest moment has to sit before the
 * estimate counts as stale and may rise despite the dwell. A fan that spun up
 * mid-call holds the level above the threshold in a way that keeps resetting
 * the dwell, so without this the gate would flap open forever; the margin
 * keeps a merely quiet speaker from tripping it.
 */
const STALE_FLOOR_MARGIN = 0.02;

/**
 * Tracks the background noise level and derives a VAD threshold from it, so
 * the user does not have to dial in a number that only holds for one room and
 * one microphone.
 *
 * The estimate drops to a quieter room within a second but climbs back at a
 * few thousandths of the scale per second, so a room that gets noisier
 * mid-call is learned over the better part of a minute. That asymmetry is the
 * whole design: being gated mid-sentence is much worse than passing noise for
 * a while longer.
 *
 * Two things stop a talking user from being gated by their own voice:
 * the floor may never rise above the quietest level of the last
 * `FLOOR_WINDOW_MS`, which speech returns to between every syllable, and it may
 * not rise at all until the input has been below the threshold for
 * `QUIET_DWELL_MS`, so the dips inside a sentence cannot ratchet it up either.
 *
 * One tracker belongs to one measurement chain; the settings meter runs its
 * own alongside the detector's. Both measure the same signal the same way
 * (`configureVadAnalyser`/`readVadLevel`), so they converge on the same
 * threshold and the marker the user sees is the one that gates their mic.
 */
export class NoiseFloorTracker {
  private floor = 0;
  private seeded = false;
  private lastUpdate = 0;
  /** When the level was last above the threshold, 0 if it never has been. */
  private lastAbove = 0;
  /** Quietest level of the current window half and of the one before it. */
  private windowMin = Infinity;
  private previousWindowMin = Infinity;
  private windowStart = 0;

  /** Current noise floor estimate on the same 0-1 scale as the level. */
  get noiseFloor(): number {
    return this.floor;
  }

  /** Threshold derived from the current estimate, clamped to the UI scale. */
  get threshold(): number {
    return Math.min(
      VAD_THRESHOLD_MAX,
      Math.max(VAD_THRESHOLD_MIN, this.floor * NOISE_FLOOR_MARGIN + NOISE_FLOOR_OFFSET)
    );
  }

  /** Feed one measurement and return the threshold to compare it against. */
  update(level: number): number {
    const now = performance.now();
    // Clamped because ticks stall while the tab is throttled or the graph is
    // suspended; a multi-second gap must not creep the floor up in one step.
    const elapsed = this.seeded ? Math.min(1, Math.max(0, (now - this.lastUpdate) / 1000)) : 0;
    this.lastUpdate = now;

    if (!this.seeded) {
      // Seeding from the first sample means starting mid-sentence puts the
      // floor at speech level; the fast downward decay corrects that within a
      // second of the first pause.
      this.floor = level;
      this.seeded = true;
      this.windowStart = now;
      this.windowMin = level;
      return this.threshold;
    }

    const recentMin = this.trackWindowMinimum(level, now);

    if (level < this.floor) {
      // Everything downwards goes through here, so the rise below only ever
      // has to raise the floor.
      this.floor += (level - this.floor) * Math.min(1, FLOOR_DECAY_PER_SECOND * elapsed);
      return this.threshold;
    }

    // The room has not actually been this quiet for a while — the estimate is
    // stale (something started making noise), so let it rise even though the
    // input still reads as speech.
    const stale = recentMin > this.floor + STALE_FLOOR_MARGIN;

    if (level > this.threshold) {
      this.lastAbove = now;
      if (!stale) return this.threshold;
    } else if (!stale && this.lastAbove !== 0 && now - this.lastAbove < QUIET_DWELL_MS) {
      // Below the threshold but above the floor: this is the gap between two
      // words far more often than it is the room getting louder, so wait until
      // the quiet has lasted longer than any such gap before believing it.
      return this.threshold;
    }

    const risen = Math.min(recentMin, this.floor + FLOOR_CREEP_PER_SECOND * elapsed);
    if (risen > this.floor) this.floor = risen;
    return this.threshold;
  }

  /**
   * Fold `level` into the rolling minimum and return the quietest level of the
   * last window. Two half-windows are kept and rotated, which costs two
   * numbers and gives a window between `FLOOR_WINDOW_MS / 2` and the full
   * `FLOOR_WINDOW_MS` — precise enough for something the floor only creeps
   * towards at a hundredth of the scale per second.
   */
  private trackWindowMinimum(level: number, now: number): number {
    this.windowMin = Math.min(this.windowMin, level);
    if (now - this.windowStart >= FLOOR_WINDOW_MS / 2) {
      this.previousWindowMin = this.windowMin;
      this.windowMin = level;
      this.windowStart = now;
    }
    return Math.min(this.windowMin, this.previousWindowMin);
  }

  /** Forget the estimate; the next sample seeds it again. */
  reset() {
    this.floor = 0;
    this.seeded = false;
    this.lastUpdate = 0;
    this.lastAbove = 0;
    this.windowMin = Infinity;
    this.previousWindowMin = Infinity;
    this.windowStart = 0;
  }
}

export class VoiceActivityDetector {
  private analyser: AnalyserNode | null = null;
  /** Node this detector listens on. Owned by the caller, never by us. */
  private source: AudioNode | null = null;
  private dataArray: Uint8Array<ArrayBuffer> | null = null;
  private stopTicks: (() => void) | null = null;
  private isActive = false;
  private currentSensitivity = 0.1;
  /** Derive the threshold from the noise floor instead of `currentSensitivity`. */
  private automatic = false;
  private noiseFloor = new NoiseFloorTracker();

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
  start(source: AudioNode, sensitivity: number = 0.1, automatic: boolean = false) {
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
      this.automatic = automatic;
      // A fresh graph means a fresh room to measure: the old estimate may come
      // from a different microphone entirely.
      this.noiseFloor.reset();
      this.source = source;
      this.source.connect(this.analyser);

      this.startAnalysis();
    } catch (error) {
      console.error('Failed to start VAD:', error);
    }
  }

  /**
   * Update sensitivity without restarting the entire VAD — the analysis loop
   * reads both fields on every tick, so nothing has to be rebuilt. Restarting
   * would drop the gate for a tick and throw away the noise-floor estimate.
   */
  updateSensitivity(sensitivity: number, automatic: boolean = this.automatic) {
    this.currentSensitivity = sensitivity;
    this.automatic = automatic;
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

    this.noiseFloor.reset();
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
  private startAnalysis() {
    if (!this.analyser || !this.dataArray) return;

    const analyze = () => {
      if (!this.analyser || !this.dataArray) return;

      const normalizedLevel = readVadLevel(this.analyser, this.dataArray);

      // In automatic mode the threshold tracks the background noise; manually
      // it is the configured sensitivity (lower = easier to trigger). The
      // tracker is fed either way so switching to automatic mid-call has an
      // estimate ready instead of seeding one from whatever is happening then.
      const autoThreshold = this.noiseFloor.update(normalizedLevel);
      const threshold = this.automatic ? autoThreshold : this.currentSensitivity;
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