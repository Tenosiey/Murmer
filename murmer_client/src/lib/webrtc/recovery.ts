/**
 * Repair policy for voice peer connections that break mid-call.
 *
 * A peer connection reports `disconnected` for any interruption long enough to
 * stall its consent checks: a Wi-Fi roam, a phone changing cell, a laptop lid
 * shut for a second. Most of those come back on their own within a few
 * seconds, so the state is not a reason to tear anything down — closing the
 * connection on it (what this module replaced) turned every brief hiccup into
 * a peer that stayed in the channel with no audio until somebody rejoined.
 * `failed` does not recover on its own, but it only condemns the current ICE
 * transport; an ICE restart gives the pair a new one over whatever path is
 * reachable now.
 *
 * So: wait a blip out, restart ICE when it does not resolve itself, keep
 * retrying until the deadline, and rebuild the connection from scratch when
 * even that does not take. The one thing never to do is leave a peer half
 * broken forever, because nothing in the UI can distinguish that from silence.
 *
 * The controller is deliberately free of `window`, WebRTC and store imports:
 * the timing rules here are what fail invisibly, so they are unit-tested
 * (`recovery.test.ts`) against fake timers instead of a live call.
 */

/** What the controller needs the transport to do on its behalf. */
export interface PeerRecoveryHooks {
  /**
   * Renegotiate ICE towards `id` on the existing connection. Only one of the
   * two ends may act on this (see `VoiceManager.restartPeerIce`), so this is a
   * no-op on the other one — the deadline still runs on both, which is what
   * lets them give up together.
   */
  restart(id: string): void;
  /**
   * Give up on the current connection to `id` and start a fresh one. Called
   * once the deadline passes, never before.
   */
  rebuild(id: string): void;
  /** Reflect an in-progress repair in the UI. */
  setReconnecting(id: string, reconnecting: boolean): void;
}

export interface PeerRecoveryTimings {
  /**
   * How long `disconnected` is left alone before it is treated as breakage.
   * Long enough to cover a roam between access points, short enough that a
   * real drop is not silent for the whole of it.
   */
  graceMs: number;
  /** Spacing between ICE restart attempts while a peer stays broken. */
  retryMs: number;
  /**
   * Total time a peer may spend broken before the connection is thrown away
   * and rebuilt. Counted from the first bad state, not from the last attempt.
   */
  deadlineMs: number;
}

export const DEFAULT_RECOVERY_TIMINGS: PeerRecoveryTimings = {
  graceMs: 3_000,
  retryMs: 5_000,
  deadlineMs: 30_000
};

interface RecoveryEntry {
  /** When this peer first went bad, for measuring the deadline. */
  startedAt: number;
  timer: ReturnType<typeof setTimeout>;
  /** Whether the pending timer is still the initial grace period. */
  waitingOutGrace: boolean;
}

export class PeerRecovery {
  private entries = new Map<string, RecoveryEntry>();

  constructor(
    private hooks: PeerRecoveryHooks,
    private timings: PeerRecoveryTimings = DEFAULT_RECOVERY_TIMINGS
  ) {}

  /** Feed a peer's connection state in. Unknown/intermediate states are ignored. */
  observe(id: string, state: RTCPeerConnectionState): void {
    switch (state) {
      case 'connected':
        this.resolve(id);
        break;
      case 'disconnected':
        this.begin(id, this.timings.graceMs);
        break;
      case 'failed':
        // Nothing to wait for: `failed` is terminal for the transport.
        this.begin(id, 0);
        break;
      case 'closed':
        // Only reachable because the connection was closed deliberately; the
        // caller owns the teardown from here.
        this.forget(id);
        break;
    }
  }

  /** Whether a repair is currently in flight for `id`. */
  isRecovering(id: string): boolean {
    return this.entries.has(id);
  }

  /** Drop all state for `id` without touching the transport or the UI. */
  forget(id: string): void {
    const entry = this.entries.get(id);
    if (!entry) return;
    clearTimeout(entry.timer);
    this.entries.delete(id);
  }

  /** Drop all state for every peer, e.g. on leaving the channel. */
  clear(): void {
    for (const entry of this.entries.values()) clearTimeout(entry.timer);
    this.entries.clear();
  }

  /** The peer came back; cancel whatever was scheduled. */
  private resolve(id: string): void {
    if (!this.entries.has(id)) return;
    this.forget(id);
    this.hooks.setReconnecting(id, false);
  }

  private begin(id: string, delay: number): void {
    const existing = this.entries.get(id);
    if (existing) {
      // `disconnected` is regularly followed by `failed` part-way through the
      // grace period. Bring the attempt forward rather than waiting out a
      // grace that now has nothing to wait for — but keep `startedAt`, so the
      // deadline still runs from when the trouble actually began.
      if (delay === 0 && existing.waitingOutGrace) {
        clearTimeout(existing.timer);
        existing.waitingOutGrace = false;
        existing.timer = setTimeout(() => this.attempt(id), 0);
      }
      return;
    }

    const entry: RecoveryEntry = {
      startedAt: Date.now(),
      waitingOutGrace: delay > 0,
      timer: setTimeout(() => this.attempt(id), delay)
    };
    this.entries.set(id, entry);
    this.hooks.setReconnecting(id, true);
  }

  private attempt(id: string): void {
    const entry = this.entries.get(id);
    if (!entry) return;

    const elapsed = Date.now() - entry.startedAt;
    if (elapsed >= this.timings.deadlineMs) {
      // Out of patience. Hand the peer back for a fresh connection instead of
      // just dropping it: the member list still shows this user in the
      // channel, and a peer we have quietly stopped trying to reach is the
      // ghost this whole module exists to avoid.
      this.entries.delete(id);
      this.hooks.setReconnecting(id, false);
      this.hooks.rebuild(id);
      return;
    }

    this.hooks.restart(id);
    entry.waitingOutGrace = false;
    // Never overshoot the deadline: the last wait is shortened so the rebuild
    // happens when it was promised rather than a retry interval later.
    const remaining = this.timings.deadlineMs - elapsed;
    const next = Math.min(this.timings.retryMs, remaining);
    entry.timer = setTimeout(() => this.attempt(id), next);
  }
}
