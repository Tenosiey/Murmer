import { getAudioContext, resumeAudioContext } from './audioContext';

/**
 * The tone a direct call plays while it rings, on both ends: the classic
 * two-tone ring cadence (440 + 480 Hz, two seconds on, four off),
 * synthesised on the shared context rather than shipped as a file. Quiet on
 * purpose — it plays through the speakers the call will use.
 */
const CYCLE_MS = 6000;
const ON_SECONDS = 2;
const LEVEL = 0.06;

let timer: ReturnType<typeof setInterval> | null = null;
/** The burst sounding now, so answering silences it at once. */
let sounding: OscillatorNode[] = [];

function burst() {
  const ctx = getAudioContext();
  if (!ctx) return;
  resumeAudioContext();
  const gain = ctx.createGain();
  const start = ctx.currentTime;
  // Ramped in and out, or every burst starts and stops with a click.
  gain.gain.setValueAtTime(0, start);
  gain.gain.linearRampToValueAtTime(LEVEL, start + 0.05);
  gain.gain.setValueAtTime(LEVEL, start + ON_SECONDS - 0.05);
  gain.gain.linearRampToValueAtTime(0, start + ON_SECONDS);
  gain.connect(ctx.destination);
  sounding = [];
  for (const frequency of [440, 480]) {
    const osc = ctx.createOscillator();
    osc.frequency.value = frequency;
    osc.connect(gain);
    osc.start(start);
    osc.stop(start + ON_SECONDS);
    sounding.push(osc);
  }
}

export function startRingtone() {
  if (timer !== null) return;
  burst();
  timer = setInterval(burst, CYCLE_MS);
}

export function stopRingtone() {
  if (timer === null) return;
  clearInterval(timer);
  timer = null;
  for (const osc of sounding) osc.stop();
  sounding = [];
}
