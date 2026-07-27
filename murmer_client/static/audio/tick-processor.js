/**
 * Audio-thread metronome for the level meters.
 *
 * `requestAnimationFrame` stops firing entirely once the window is minimised
 * or hidden, which froze voice-activity detection in whatever state it last
 * saw — an open microphone that never closed, or a closed one that could
 * never open. Audio worklets run on the audio rendering thread, which keeps
 * running regardless of window state, and the messages they post are not
 * subject to the timer throttling hidden pages get.
 *
 * The processor deliberately does no measuring of its own: the analysers stay
 * on the main thread so the level stays exactly the value the VAD threshold
 * was calibrated against. This only says "now would be a good time to read
 * them".
 */
class TickProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    // One render quantum is 128 frames; at 48 kHz that is ~2.67 ms.
    const interval = options?.processorOptions?.intervalMs ?? 16;
    this.quantaPerTick = Math.max(1, Math.round((interval * sampleRate) / 128000));
    this.counter = 0;
    this.active = true;
    // Disconnecting the node does not stop the graph from pulling it, so
    // going idle is a flag rather than a graph change.
    this.port.onmessage = (event) => {
      this.active = event.data?.active !== false;
      this.counter = 0;
    };
  }

  process() {
    // Returning true keeps the node alive: it is connected to a muted gain
    // node purely so the graph keeps calling this.
    if (!this.active) return true;
    if (++this.counter >= this.quantaPerTick) {
      this.counter = 0;
      this.port.postMessage(0);
    }
    return true;
  }
}

registerProcessor('tick-processor', TickProcessor);
