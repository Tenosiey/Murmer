/**
 * Audio-thread tap for the microphone test.
 *
 * Copies the input to the main thread so a few seconds of it can be played
 * back. Raw samples are handed over rather than an encoded recording because
 * the whole point of the test is to hear what echo cancellation, noise
 * suppression and automatic gain control did to the voice — a lossy encode in
 * between would add artifacts of its own to the thing being judged.
 *
 * Samples are batched instead of posted per render quantum: a quantum is 128
 * frames, which at 48 kHz would be ~375 messages a second for the whole
 * recording.
 */
const CHUNK_FRAMES = 4096;

class RecorderProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.buffer = new Float32Array(CHUNK_FRAMES);
    this.offset = 0;
  }

  process(inputs) {
    // Only the first channel: the capture is mono in practice, and a test
    // recording is about the processing, not the channel layout.
    const channel = inputs[0]?.[0];
    // No connected input yet (or the source ended) — keep the node alive.
    if (!channel || channel.length === 0) return true;

    for (let i = 0; i < channel.length; i++) {
      this.buffer[this.offset++] = channel[i];
      if (this.offset === CHUNK_FRAMES) {
        this.port.postMessage(this.buffer.slice(0, this.offset));
        this.offset = 0;
      }
    }
    return true;
  }
}

registerProcessor('recorder-processor', RecorderProcessor);
