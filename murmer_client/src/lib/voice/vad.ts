/**
 * Voice Activity Detection (VAD) utility
 * Monitors audio levels to detect when the user is speaking
 */

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

export class VoiceActivityDetector {
  private audioContext: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private dataArray: Uint8Array<ArrayBuffer> | null = null;
  private animationFrame: number | null = null;
  private isActive = false;
  private currentSensitivity = 0.1;
  private currentStream: MediaStream | null = null;
  
  // Debouncing for voice activity
  private lastVoiceTime = 0;
  private holdTime = 800; // Hold transmission for 800ms after voice stops
  private releaseTimeout: number | null = null;
  
  private listeners: Array<(isActive: boolean, level: number) => void> = [];

  constructor() {
    this.setupAudioContext();
  }

  private setupAudioContext() {
    try {
      this.audioContext = new AudioContext();
      this.analyser = this.audioContext.createAnalyser();
      this.dataArray = configureVadAnalyser(this.analyser);
    } catch (error) {
      console.error('Failed to setup audio context for VAD:', error);
    }
  }

  /**
   * Start monitoring the given audio stream for voice activity
   */
  start(stream: MediaStream, sensitivity: number = 0.1) {
    if (!this.audioContext || !this.analyser || !this.dataArray) {
      console.error('Audio context not initialized');
      return;
    }

    // Stop any existing monitoring first
    this.stop();

    try {
      // Resume audio context if suspended (required by some browsers)
      if (this.audioContext.state === 'suspended') {
        this.audioContext.resume();
      }

      this.currentStream = stream;
      this.currentSensitivity = sensitivity;
      this.source = this.audioContext.createMediaStreamSource(stream);
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
    if (this.currentStream && this.source) {
      this.start(this.currentStream, sensitivity);
    }
  }

  /**
   * Set the hold time (how long to keep transmission active after voice stops)
   */
  setHoldTime(milliseconds: number) {
    this.holdTime = Math.max(0, milliseconds);
  }

  /**
   * Stop monitoring voice activity
   */
  stop() {
    if (this.animationFrame) {
      cancelAnimationFrame(this.animationFrame);
      this.animationFrame = null;
    }

    if (this.releaseTimeout) {
      clearTimeout(this.releaseTimeout);
      this.releaseTimeout = null;
    }

    if (this.source) {
      this.source.disconnect();
      this.source = null;
    }

    this.currentStream = null;
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
      
      // Update last voice time if we detect voice
      if (rawVoiceDetected) {
        this.lastVoiceTime = currentTime;
        
        // Clear any pending release timeout
        if (this.releaseTimeout) {
          clearTimeout(this.releaseTimeout);
          this.releaseTimeout = null;
        }
        
        // Immediately activate if not already active
        if (!this.isActive) {
          this.isActive = true;
          this.notifyListeners(true, normalizedLevel);
        }
      } else {
        // No voice detected, but check if we're within hold time
        const timeSinceLastVoice = currentTime - this.lastVoiceTime;
        const shouldHold = timeSinceLastVoice < this.holdTime;
        
        if (this.isActive && !shouldHold && !this.releaseTimeout) {
          // Start release timeout
          this.releaseTimeout = window.setTimeout(() => {
            this.isActive = false;
            this.releaseTimeout = null;
            this.notifyListeners(false, normalizedLevel);
          }, 100); // Small delay to prevent rapid toggling
        }
      }

      this.animationFrame = requestAnimationFrame(analyze);
    };

    analyze();
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
    if (this.audioContext && this.audioContext.state !== 'closed') {
      this.audioContext.close();
    }
    this.listeners = [];
  }
}