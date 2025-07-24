/**
 * Voice Activity Detection (VAD) utility
 * Monitors audio levels to detect when the user is speaking
 */

export class VoiceActivityDetector {
  private audioContext: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private dataArray: Uint8Array | null = null;
  private animationFrame: number | null = null;
  private isActive = false;
  private currentSensitivity = 0.1;
  private currentStream: MediaStream | null = null;
  
  // Debouncing for voice activity
  private lastVoiceTime = 0;
  private holdTime = 800; // Hold transmission for 800ms after voice stops
  private releaseTimeout: number | null = null;
  
  private readonly SMOOTHING_FACTOR = 0.3;
  private readonly FFT_SIZE = 256;
  private readonly MIN_DECIBELS = -90;
  private readonly MAX_DECIBELS = -10;
  
  private listeners: Array<(isActive: boolean, level: number) => void> = [];

  constructor() {
    this.setupAudioContext();
  }

  private setupAudioContext() {
    try {
      this.audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
      this.analyser = this.audioContext.createAnalyser();
      this.analyser.fftSize = this.FFT_SIZE;
      this.analyser.minDecibels = this.MIN_DECIBELS;
      this.analyser.maxDecibels = this.MAX_DECIBELS;
      this.analyser.smoothingTimeConstant = this.SMOOTHING_FACTOR;
      
      this.dataArray = new Uint8Array(this.analyser.frequencyBinCount);
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

      this.analyser.getByteFrequencyData(this.dataArray);
      
      // Calculate average volume across frequency spectrum
      let sum = 0;
      for (let i = 0; i < this.dataArray.length; i++) {
        sum += this.dataArray[i];
      }
      const average = sum / this.dataArray.length;
      
      // Normalize to 0-1 range
      const normalizedLevel = average / 255;
      
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

    this.analyser.getByteFrequencyData(this.dataArray);
    let sum = 0;
    for (let i = 0; i < this.dataArray.length; i++) {
      sum += this.dataArray[i];
    }
    return (sum / this.dataArray.length) / 255;
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