/**
 * Which transport carries a voice call — the peer-to-peer mesh or the
 * server's SFU — and how a call moves between them
 * (`docs/voice.md`).
 *
 * The server decides the mode per channel and announces it with
 * `voice-mode` (and in the `voice-permissions` reply to a join). The client
 * only follows, but following has to be **make before break**: the old
 * transport keeps carrying the call until the new one is up, or a switch at
 * the seventh join would cut everybody's audio for the length of an ICE
 * handshake. The rules for that live here as a pure function, because a
 * wrong step fails silently — a call that never leaves the mesh, or two
 * transports playing the same voice twice — and nothing in the UI shows it.
 *
 * The frame parsers are here too: both frames are untrusted input, and a
 * malformed slot map would otherwise attach somebody's audio to the wrong
 * member.
 */

export type VoiceTransport = 'mesh' | 'sfu';

/** The channel's mode as the server last announced it. */
export interface VoiceModeFrame {
  channelId: number;
  mode: VoiceTransport;
  /** Receive slots the SFU offer must carry: one per other possible member. */
  slots: number;
}

/** One receive transceiver of the SFU connection and whose media it carries. */
export interface SfuSlot {
  mid: string;
  user: string;
  kind: 'audio' | 'video';
}

/**
 * The server never sizes a slot map past the channel cap, and channel limits
 * stop at 99, so anything larger is a broken or hostile frame rather than a
 * reason to create thousands of transceivers.
 */
export const MAX_SFU_SLOTS = 99;

/**
 * Parse a `voice-mode` or `voice-permissions` frame. `null` when it carries
 * no usable mode — a server that predates the SFU sends no `mode` at all,
 * which is the mesh.
 */
export function parseVoiceMode(msg: Record<string, unknown>): VoiceModeFrame | null {
  const { channelId, mode, slots } = msg;
  if (typeof channelId !== 'number' || !Number.isInteger(channelId)) return null;
  if (mode !== 'mesh' && mode !== 'sfu') return null;
  const count =
    typeof slots === 'number' && Number.isInteger(slots) && slots >= 0 && slots <= MAX_SFU_SLOTS
      ? slots
      : 0;
  // An SFU offer with no receive slots could hear nobody: the server sends
  // `0` only while the SFU is off, so this is not a mode we can follow.
  if (mode === 'sfu' && count === 0) return null;
  return { channelId, mode, slots: count };
}

/**
 * Parse an `sfu-slots` frame. Rejects the whole frame rather than keeping
 * the entries that look right: a map with one bad entry is a map from a
 * server this client does not understand, and half of it is no safer.
 */
export function parseSfuSlots(
  msg: Record<string, unknown>
): { channelId: number; slots: SfuSlot[] } | null {
  const { channelId, slots } = msg;
  if (typeof channelId !== 'number' || !Number.isInteger(channelId)) return null;
  if (!Array.isArray(slots) || slots.length > MAX_SFU_SLOTS * 2) return null;
  const parsed: SfuSlot[] = [];
  const mids = new Set<string>();
  for (const entry of slots) {
    if (typeof entry !== 'object' || entry === null) return null;
    const { mid, user, kind } = entry as Record<string, unknown>;
    if (typeof mid !== 'string' || mid === '' || mids.has(mid)) return null;
    if (typeof user !== 'string' || user === '') return null;
    if (kind !== 'audio' && kind !== 'video') return null;
    mids.add(mid);
    parsed.push({ mid, user, kind });
  }
  return { channelId, slots: parsed };
}

/** Where a call stands between the two transports. */
export interface TransportState {
  /** What the server last said the channel should use. */
  target: VoiceTransport;
  /** What the UI shows and plays right now. */
  live: VoiceTransport;
  /** Whether an SFU connection exists, live or not. */
  sfuOpen: boolean;
}

export type TransportEvent =
  | { type: 'mode'; mode: VoiceTransport }
  /** The SFU connection reached `connected`. */
  | { type: 'sfu-connected' }
  /** Every mesh peer connected, or the wait for them ran out. */
  | { type: 'mesh-ready' };

export type TransportAction =
  /** Offer a new SFU connection. */
  | 'open-sfu'
  /** Close the SFU connection. */
  | 'close-sfu'
  /** Offer mesh connections to the members (and start the wait for them). */
  | 'build-mesh'
  /** Close every mesh connection. */
  | 'close-mesh';

export const INITIAL_TRANSPORT: TransportState = { target: 'mesh', live: 'mesh', sfuOpen: false };

/**
 * The next state, and what to do to get there.
 *
 * The live transport only changes once the other one is ready. Until then a
 * change of mind costs nothing: the half-built side is closed and the live
 * one never stopped.
 */
export function stepTransport(
  state: TransportState,
  event: TransportEvent
): { state: TransportState; actions: TransportAction[] } {
  switch (event.type) {
    case 'mode': {
      const target = event.mode;
      if (target === state.target) return { state, actions: [] };
      if (target === 'sfu') {
        if (state.live === 'sfu') {
          // Flipped back before the mesh took over: drop the half-built mesh.
          return { state: { ...state, target }, actions: ['close-mesh'] };
        }
        return { state: { ...state, target, sfuOpen: true }, actions: ['open-sfu'] };
      }
      if (state.live === 'mesh') {
        // The SFU never went live (or was never opened): nothing to wait for.
        return {
          state: { ...state, target, sfuOpen: false },
          actions: state.sfuOpen ? ['close-sfu'] : []
        };
      }
      return { state: { ...state, target }, actions: ['build-mesh'] };
    }
    case 'sfu-connected':
      if (state.target !== 'sfu' || state.live === 'sfu') return { state, actions: [] };
      return { state: { ...state, live: 'sfu' }, actions: ['close-mesh'] };
    case 'mesh-ready':
      if (state.target !== 'mesh' || state.live === 'mesh') return { state, actions: [] };
      return { state: { ...state, live: 'mesh', sfuOpen: false }, actions: ['close-sfu'] };
  }
}
