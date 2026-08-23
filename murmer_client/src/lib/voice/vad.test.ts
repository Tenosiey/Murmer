import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  FLOOR_CREEP_PER_SECOND,
  FLOOR_DECAY_PER_SECOND,
  FLOOR_WINDOW_MS,
  NoiseFloorTracker,
  QUIET_DWELL_MS,
  ReleaseGate,
  STALE_FLOOR_MARGIN,
  VAD_THRESHOLD_MAX,
  VAD_THRESHOLD_MIN
} from './vad';
import {
  clampVadRelease,
  VAD_RELEASE_DEFAULT_MS,
  VAD_RELEASE_MAX_MS,
  VAD_RELEASE_MIN_MS
} from '../stores/settings';

/**
 * The gate's timings are invisible in the UI — a release applied a burst too
 * late, or one that ignores a change made mid-sentence, still looks and sounds
 * like a working microphone to the person holding it. Hence a fake clock.
 */
describe('ReleaseGate', () => {
  it('opens on the first sample above the threshold', () => {
    const gate = new ReleaseGate();
    expect(gate.update(false, 0, 900)).toBe(false);
    expect(gate.update(true, 16, 900)).toBe(true);
    expect(gate.isOpen).toBe(true);
  });

  it('holds through a pause and closes once the release has elapsed', () => {
    const gate = new ReleaseGate();
    gate.update(true, 1000, 900);
    expect(gate.update(false, 1500, 900)).toBe(true);
    expect(gate.update(false, 1899, 900)).toBe(true);
    expect(gate.update(false, 1900, 900)).toBe(false);
  });

  it('re-arms the release on every burst of speech', () => {
    const gate = new ReleaseGate();
    gate.update(true, 0, 900);
    gate.update(false, 800, 900);
    // A word lands inside the release window: the gate must stay open for a
    // full release measured from *this* word, not from the previous one.
    gate.update(true, 850, 900);
    expect(gate.update(false, 1700, 900)).toBe(true);
    expect(gate.update(false, 1750, 900)).toBe(false);
  });

  it('applies a release changed mid-release instead of the one in flight', () => {
    const gate = new ReleaseGate();
    gate.update(true, 0, 2000);
    expect(gate.update(false, 500, 2000)).toBe(true);
    // Slider dragged down while the gate is coasting: the shorter release is
    // already past, so it closes on this tick rather than at the old deadline.
    expect(gate.update(false, 516, 200)).toBe(false);
  });

  it('closes on the first quiet sample with no release at all', () => {
    const gate = new ReleaseGate();
    gate.update(true, 0, 0);
    expect(gate.update(false, 16, 0)).toBe(false);
  });

  it('closes and forgets the last burst on reset', () => {
    const gate = new ReleaseGate();
    gate.update(true, 1000, 900);
    gate.reset();
    expect(gate.isOpen).toBe(false);
    // Without forgetting `lastAbove`, a restart whose clock is past the old
    // deadline would report the gate open for one tick.
    expect(gate.update(false, 5000, 900)).toBe(false);
  });
});

describe('clampVadRelease', () => {
  it('keeps a value inside the scale, rounded to whole milliseconds', () => {
    expect(clampVadRelease(450.4)).toBe(450);
    expect(clampVadRelease(VAD_RELEASE_DEFAULT_MS)).toBe(VAD_RELEASE_DEFAULT_MS);
  });

  it('clamps a stored value from outside the scale to its ends', () => {
    expect(clampVadRelease(-1)).toBe(VAD_RELEASE_MIN_MS);
    expect(clampVadRelease(60_000)).toBe(VAD_RELEASE_MAX_MS);
  });

  it('falls back to the default for a value that is not a number', () => {
    expect(clampVadRelease(Number.NaN)).toBe(VAD_RELEASE_DEFAULT_MS);
    expect(clampVadRelease(Number.POSITIVE_INFINITY)).toBe(VAD_RELEASE_DEFAULT_MS);
  });
});

/**
 * The noise floor is the input to a threshold nobody ever sees a number for:
 * when it drifts too high the microphone closes in the middle of a word, and
 * the person holding it hears nothing wrong at all. Its two guarantees — the
 * floor may never rise above the quietest level of the tracked window, and may
 * not rise at all until the input has been quiet for `QUIET_DWELL_MS` — are
 * therefore checked as properties over generated microphone input rather than
 * against a handful of hand-picked sequences: a regression that only shows up
 * after the fortieth syllable of one particular sentence is exactly the kind
 * an example test walks past.
 *
 * The generators are seeded, so a failure names a seed that reproduces it. The
 * tracker reads `performance.now()` itself, which is why the clock is faked
 * rather than injected.
 */
describe('NoiseFloorTracker', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['performance'] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  /** One microphone tick: how long since the previous one, and what it read. */
  interface Step {
    dt: number;
    level: number;
    /** Whether this tick is somebody talking, for the gating assertions. */
    speech?: boolean;
  }

  interface Observation extends Step {
    index: number;
    now: number;
    floor: number;
    floorBefore: number;
    threshold: number;
  }

  /**
   * Deterministic PRNG (mulberry32). A seeded generator beats `Math.random()`
   * here for the same reason these tests exist: the failures worth catching
   * are the ones that need a specific sequence, and a failure nobody can
   * replay is not much of a test.
   */
  function mulberry32(seed: number): () => number {
    let a = seed >>> 0;
    return () => {
      a = (a + 0x6d2b79f5) >>> 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  /** Replay a tick sequence through a fresh tracker, observing every step. */
  function run(steps: Step[], observe: (o: Observation) => void = () => {}): NoiseFloorTracker {
    const tracker = new NoiseFloorTracker();
    let floorBefore = 0;
    steps.forEach((step, index) => {
      vi.advanceTimersByTime(step.dt);
      const threshold = tracker.update(step.level);
      observe({
        ...step,
        index,
        now: performance.now(),
        floor: tracker.noiseFloor,
        floorBefore,
        threshold
      });
      floorBefore = tracker.noiseFloor;
    });
    return tracker;
  }

  /**
   * Levels a microphone actually produces: mostly room noise, speech on top of
   * it, the odd stretch of near-silence and the occasional outlier — plus
   * ticks that arrive seconds late, because the audio graph stalls while the
   * tab is hidden and the tracker's own clamp on that gap is part of what is
   * under test.
   */
  function randomTicks(seed: number, count: number): Step[] {
    const rand = mulberry32(seed);
    const room = rand() * 0.12;
    const steps: Step[] = [];
    for (let i = 0; i < count; i++) {
      const roll = rand();
      const level =
        roll < 0.55
          ? room + rand() * 0.02
          : roll < 0.85
            ? room + 0.1 + rand() * 0.5
            : roll < 0.95
              ? rand() * room
              : rand();
      const dt = rand() < 0.06 ? Math.round(rand() * 30_000) : 10 + Math.round(rand() * 50);
      steps.push({ dt, level });
    }
    return steps;
  }

  /**
   * Somebody talking: a lead-in of room noise, then syllables separated by the
   * gaps speech actually returns to the room in. Every gap is shorter than
   * `QUIET_DWELL_MS`, so none of them may count as "the room is quiet now" —
   * this is the sequence that used to ratchet the threshold up under the
   * speaker until it overtook them.
   */
  function sentence(seed: number): { steps: Step[]; room: number } {
    const rand = mulberry32(seed);
    const room = 0.01 + rand() * 0.12;
    const speech = room + 0.18 + rand() * 0.3;
    const tick = 15 + Math.round(rand() * 35);
    const steps: Step[] = [];
    const hold = (ms: number, level: () => number, isSpeech = false) => {
      for (let t = 0; t < ms; t += tick) steps.push({ dt: tick, level: level(), speech: isSpeech });
    };

    // Seed the floor on the room rather than mid-word.
    hold(1000, () => room + rand() * 0.008);
    const syllables = 20 + Math.floor(rand() * 40);
    for (let i = 0; i < syllables; i++) {
      hold(120 + Math.round(rand() * 300), () => speech + rand() * 0.1, true);
      hold(40 + Math.round(rand() * (QUIET_DWELL_MS - 1100)), () => room + rand() * 0.008);
    }
    return { steps, room };
  }

  /**
   * A room that got louder for good: a quiet moment to seed the floor on, then
   * a fan that spun up and never dips again. This is the only shape in which
   * the floor rises at all — a speaker returns to the room between syllables
   * and a quiet room gives the floor nothing to climb towards — so everything
   * about rising has to be tested through it.
   */
  function roomGotLouder(seed: number): { steps: Step[]; loud: number } {
    const rand = mulberry32(seed);
    const quiet = 0.01 + rand() * 0.04;
    const loud = quiet + STALE_FLOOR_MARGIN + 0.1 + rand() * 0.25;
    const steps: Step[] = [];
    for (let t = 0; t < 1000; t += 50) steps.push({ dt: 50, level: quiet });
    for (let t = 0; t < 180_000; t += 50) steps.push({ dt: 50, level: loud });
    return { steps, loud };
  }

  /**
   * The window is kept as two rotating halves, so what it actually covers is
   * somewhere between `FLOOR_WINDOW_MS / 2` and the full `FLOOR_WINDOW_MS` of
   * history. The half is therefore the span the guarantee holds for
   * unconditionally, and the one the test can assert against.
   */
  const GUARANTEED_WINDOW_MS = FLOOR_WINDOW_MS / 2;

  it('never raises the floor above the quietest level of the tracked window', () => {
    for (let seed = 1; seed <= 200; seed++) {
      const history: Array<{ t: number; level: number }> = [];
      run(randomTicks(seed, 300), o => {
        history.push({ t: o.now, level: o.level });
        // The first sample seeds the estimate outright; only rises after that
        // are bounded, and a fall may well leave the floor above a level the
        // input has already dropped to — that is the decay, not a ratchet.
        if (o.index === 0 || o.floor <= o.floorBefore) return;
        let windowMin = Infinity;
        for (const sample of history) {
          if (sample.t >= o.now - GUARANTEED_WINDOW_MS) {
            windowMin = Math.min(windowMin, sample.level);
          }
        }
        expect(o.floor, `seed ${seed}, step ${o.index}`).toBeLessThanOrEqual(windowMin);
      });
    }
  });

  it('never raises the floor faster than the creep rate, however late a tick is', () => {
    for (let seed = 1; seed <= 200; seed++) {
      run(randomTicks(seed, 300), o => {
        if (o.index === 0) return;
        // The tracker clamps the gap to a second, so a tick that arrives half
        // a minute late must not creep the floor up half a minute's worth.
        const allowed = FLOOR_CREEP_PER_SECOND * Math.min(1, o.dt / 1000);
        expect(o.floor - o.floorBefore, `seed ${seed}, step ${o.index}`).toBeLessThanOrEqual(
          allowed + 1e-12
        );
      });
    }
  });

  it('keeps the derived threshold on the scale the slider and meter are drawn on', () => {
    for (let seed = 1; seed <= 200; seed++) {
      run(randomTicks(seed, 300), o => {
        const where = `seed ${seed}, step ${o.index}`;
        expect(o.threshold, where).toBeGreaterThanOrEqual(VAD_THRESHOLD_MIN);
        expect(o.threshold, where).toBeLessThanOrEqual(VAD_THRESHOLD_MAX);
        expect(Number.isFinite(o.floor), where).toBe(true);
      });
    }
  });

  it('does not let a speaker ratchet the floor up with their own voice', () => {
    for (let seed = 1; seed <= 60; seed++) {
      const { steps, room } = sentence(seed);
      const tracker = run(steps, o => {
        if (o.index === 0) return;
        // Every gap in the sentence is shorter than the dwell, so nothing in
        // here may count as quiet: the floor may fall, never rise.
        expect(o.floor, `seed ${seed}, step ${o.index}`).toBeLessThanOrEqual(o.floorBefore);
      });
      expect(tracker.noiseFloor, `seed ${seed}`).toBeLessThanOrEqual(room + 0.008);
    }
  });

  it('keeps every syllable of a long sentence above the threshold', () => {
    for (let seed = 1; seed <= 60; seed++) {
      // The consequence the two guarantees exist for: being cut off mid-word.
      run(sentence(seed).steps, o => {
        if (o.speech) expect(o.level, `seed ${seed}, step ${o.index}`).toBeGreaterThan(o.threshold);
      });
    }
  });

  it('follows a room that gets quieter within a second', () => {
    for (let seed = 1; seed <= 30; seed++) {
      const rand = mulberry32(seed);
      const loud = 0.1 + rand() * 0.35;
      const quiet = rand() * (loud - 0.05);
      const tick = 20;
      const steps: Step[] = [];
      for (let t = 0; t < 500; t += tick) steps.push({ dt: tick, level: loud });
      for (let t = 0; t < 1000; t += tick) steps.push({ dt: tick, level: quiet });

      const tracker = run(steps);
      // Discrete decay can only lag the continuous one it approximates, so
      // whatever the tick rate, a second leaves at most e^-rate of the gap.
      const remaining = Math.exp(-FLOOR_DECAY_PER_SECOND) * (loud - quiet);
      expect(tracker.noiseFloor - quiet, `seed ${seed}`).toBeLessThanOrEqual(remaining);
      // ...and it approaches from above: undershooting would pass room noise.
      expect(tracker.noiseFloor, `seed ${seed}`).toBeGreaterThan(quiet);
    }
  });

  it('still learns a fan that spun up mid-call and never dips', () => {
    for (let seed = 1; seed <= 8; seed++) {
      // Without the stale-estimate escape the fan would keep re-arming the
      // dwell, the floor would stay at the old room level and the gate would
      // hold open on nothing but fan noise for the rest of the call.
      const { steps, loud } = roomGotLouder(seed);
      expect(run(steps).threshold, `seed ${seed}`).toBeGreaterThanOrEqual(loud);
    }
  });

  it('starts over from the next sample after a reset', () => {
    for (let seed = 1; seed <= 12; seed++) {
      // A reset happens when the graph is rebuilt around a different
      // microphone, so the old one is put in a far quieter room than the new
      // one: a window half left behind there would cap the new room's floor
      // at a level it never produced, and the floor is what decides how loud
      // the room may get before the gate stops opening on it.
      const rand = mulberry32(seed + 5000);
      const reused = new NoiseFloorTracker();
      for (let t = 0; t < 30_000; t += 50) {
        vi.advanceTimersByTime(50);
        reused.update(rand() * 0.002);
      }
      reused.reset();

      // Replayed through the shape that makes the floor rise, because the
      // leftovers only ever show up on the way up.
      const after = roomGotLouder(seed).steps;
      const replay = (tracker: NoiseFloorTracker) =>
        after.map(step => {
          vi.advanceTimersByTime(step.dt);
          return tracker.update(step.level);
        });
      expect(replay(reused), `seed ${seed}`).toEqual(replay(new NoiseFloorTracker()));
    }
  });

  it('keeps the longest release the user can pick shorter than the dwell', () => {
    // A release that outlasted the dwell would let the floor start rising
    // while the gate is still coasting — the ratchet these guarantees prevent.
    expect(VAD_RELEASE_MAX_MS).toBeLessThan(QUIET_DWELL_MS);
  });
});
