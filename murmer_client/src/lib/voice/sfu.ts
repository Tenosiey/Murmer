/**
 * The voice connection to the server's SFU, used while a channel is in SFU
 * mode (`voice/mode.ts`, `plans/hybrid-voice-sfu.md`).
 *
 * One `RTCPeerConnection` to the server instead of one per member, offered
 * **once and never renegotiated**: a send-only audio and video transceiver
 * for ourselves, and a receive-only audio + video pair per other member the
 * channel could ever hold (`slots`). The server then only says who is in
 * which slot (`sfu-slots`), so a join, a leave or a camera toggle is a slot
 * reassignment or a `replaceTrack`, never an offer — the same reason the
 * mesh pre-negotiates its camera transceiver, and with the same payoff: no
 * glare to handle. An idle receive transceiver costs nothing on the wire.
 *
 * Its peers come out in the mesh's `RemotePeer` shape (`id` = account name),
 * so remote audio, speaking rings, camera tiles and the connection bars do
 * not know which transport carries the call.
 *
 * The server is ICE-lite with one public host candidate and learns our
 * address from our own connectivity checks, so we never trickle candidates
 * and need no STUN here.
 */
import type { RemotePeer } from '../types';
import { capBitrate } from '../webrtc/bitrate';
import { withOpusFeatures } from './sdp';
import type { SfuSlot } from './mode';

export interface SfuConnectionOptions {
  channelId: number;
  /** Receive slots to offer, from the `voice-mode` frame. */
  slots: number;
  /** The gated microphone track — the same one every mesh peer gets. */
  audio: MediaStreamTrack | null;
  camera: MediaStreamTrack | null;
  audioBitrate: number | null;
  /** Send a frame to the server. */
  send: (frame: Record<string, unknown>) => void;
  /** The peer list changed. */
  onPeers: () => void;
  /** The connection's state changed. */
  onState: (state: RTCPeerConnectionState) => void;
  /** Applies the camera's encoder cap to the video sender. */
  configureVideo: (sender: RTCRtpSender) => void;
}

/** A member's media as the SFU delivers it, and which slot it rides. */
interface SlotPeer {
  peer: RemotePeer;
  audioMid?: string;
  videoMid?: string;
}

export class SfuConnection {
  readonly pc: RTCPeerConnection;
  private audioSender: RTCRtpSender;
  private videoSender: RTCRtpSender;
  /** Receive track of each slot transceiver, by mid. */
  private tracks = new Map<string, MediaStreamTrack>();
  /** The slot map the server last sent; applied once the tracks exist. */
  private slots: SfuSlot[] = [];
  private members = new Map<string, SlotPeer>();
  private closed = false;

  constructor(private options: SfuConnectionOptions) {
    // One transport for everything: the SFU only speaks BUNDLE.
    this.pc = new RTCPeerConnection({ bundlePolicy: 'max-bundle' });
    this.audioSender = this.pc.addTransceiver(options.audio ?? 'audio', {
      direction: 'sendonly'
    }).sender;
    capBitrate(this.audioSender, options.audioBitrate);
    this.videoSender = this.pc.addTransceiver(options.camera ?? 'video', {
      direction: 'sendonly'
    }).sender;
    options.configureVideo(this.videoSender);
    // The server pairs the n-th receive audio with the n-th receive video,
    // in offer order, so they are added strictly in pairs.
    for (let i = 0; i < options.slots; i++) {
      this.pc.addTransceiver('audio', { direction: 'recvonly' });
      this.pc.addTransceiver('video', { direction: 'recvonly' });
    }
    this.pc.ontrack = (ev) => {
      const mid = ev.transceiver.mid;
      if (mid === null) return;
      this.tracks.set(mid, ev.track);
      this.applySlots();
    };
    this.pc.onconnectionstatechange = () => {
      if (!this.closed) options.onState(this.pc.connectionState);
    };
  }

  /** Send the one offer this connection makes (or an ICE restart of it). */
  async offer(iceRestart = false) {
    const offer = withOpusFeatures(await this.pc.createOffer({ iceRestart }));
    await this.pc.setLocalDescription(offer);
    if (this.closed) return;
    this.options.send({
      type: 'sfu-offer',
      channelId: this.options.channelId,
      sdp: offer.sdp
    });
  }

  /** Apply the server's `sfu-answer`. */
  async answer(sdp: string) {
    if (this.closed || this.pc.signalingState !== 'have-local-offer') return;
    // Munged on the way in too: the remote description is what configures
    // our encoder's DTX and FEC (see `sdp.ts`).
    await this.pc.setRemoteDescription(withOpusFeatures({ type: 'answer', sdp }));
  }

  /** Apply the server's `sfu-slots`. */
  setSlots(slots: SfuSlot[]) {
    this.slots = slots;
    this.applySlots();
  }

  /**
   * Rebuild the member list from the slot map. A member whose slot did not
   * move keeps their `RemotePeer` and its streams: handing an `<audio>` or
   * `<video>` element a new stream restarts it.
   */
  private applySlots() {
    const next = new Map<string, SlotPeer>();
    for (const slot of this.slots) {
      const track = this.tracks.get(slot.mid);
      if (!track) continue;
      const old = this.members.get(slot.user);
      const entry = next.get(slot.user) ?? {
        peer: { id: slot.user, stream: new MediaStream(), reconnecting: old?.peer.reconnecting }
      };
      if (slot.kind === 'audio') {
        entry.audioMid = slot.mid;
        entry.peer.stream =
          old?.audioMid === slot.mid ? old.peer.stream : new MediaStream([track]);
      } else {
        entry.videoMid = slot.mid;
        entry.peer.video = old?.videoMid === slot.mid ? old.peer.video : new MediaStream([track]);
      }
      if (old) entry.peer.stats = old.peer.stats;
      next.set(slot.user, entry);
    }
    this.members = next;
    this.options.onPeers();
  }

  peers(): RemotePeer[] {
    return [...this.members.values()].map((m) => m.peer);
  }

  /** Who the audio slot `mid` currently carries, for per-member stats. */
  audioUser(mid: string): string | undefined {
    for (const [user, m] of this.members) if (m.audioMid === mid) return user;
    return undefined;
  }

  setReconnecting(reconnecting: boolean) {
    for (const m of this.members.values()) m.peer.reconnecting = reconnecting;
    this.options.onPeers();
  }

  setCamera(track: MediaStreamTrack | null) {
    this.videoSender.replaceTrack(track).catch((error) => {
      console.error('Failed to update the camera sent to the server:', error);
    });
    if (track) this.options.configureVideo(this.videoSender);
  }

  setAudioBitrate(bitrate: number | null) {
    capBitrate(this.audioSender, bitrate);
  }

  close() {
    this.closed = true;
    this.pc.close();
    this.members.clear();
  }
}
