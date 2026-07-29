/**
 * WebRTC screen sharing manager.
 *
 * Handles peer-to-peer screen sharing connections using WebRTC. Allows users
 * to share their screen with configurable quality and frame rate settings.
 * Uses the same signaling infrastructure as voice chat.
 */
import { chat } from '../stores/chat';
import { remoteFingerprint } from '../webrtc/fingerprint';
import { PeerRecovery } from '../webrtc/recovery';
import type { Message, ScreenShareSettings, ScreenSharePeer } from '../types';

const DEFAULT_SETTINGS: ScreenShareSettings = {
  width: 1280,
  height: 720,
  frameRate: 30,
  maxBitrate: 5_000_000
};

// Resolution + bitrate only: the frame rate is a separate user setting and
// deliberately not part of the presets.
export const QUALITY_PRESETS = {
  '480p': { width: 854, height: 480, maxBitrate: 1_500_000 },
  '720p': { width: 1280, height: 720, maxBitrate: 5_000_000 },
  '1080p': { width: 1920, height: 1080, maxBitrate: 8_000_000 },
  '1440p': { width: 2560, height: 1440, maxBitrate: 15_000_000 },
  '4k': { width: 3840, height: 2160, maxBitrate: 25_000_000 }
} as const;

export type QualityPreset = keyof typeof QUALITY_PRESETS;

/**
 * Which end of a connection a signaling frame comes from. Two users can share
 * *and* watch each other at the same time, so a peer's name alone does not
 * identify a connection — the frames say which of the two they belong to.
 */
export type ScreenShareRole = 'viewer' | 'sharer';

/**
 * How many times a share we are watching may be rebuilt from scratch before it
 * is treated as gone. Unlike a voice peer — a row in a member list — a share is
 * a window on the viewer's screen, and one that says "Reconnecting" forever is
 * worse than one that closes. Three rebuilds is roughly a minute and a half of
 * a share that never comes back.
 */
const MAX_REBUILDS = 3;

/**
 * The key a connection is repaired under. A pair of users can share to each
 * other at the same time, so the two connections with the same peer are told
 * apart by our own role on them, exactly as the signaling frames are.
 */
function recoveryKey(role: ScreenShareRole, remote: string): string {
  return `${role}:${remote}`;
}

/** The role and peer a `recoveryKey` was built from. */
function splitKey(key: string): [ScreenShareRole, string] {
  const separator = key.indexOf(':');
  return [key.slice(0, separator) as ScreenShareRole, key.slice(separator + 1)];
}

export class ScreenShareManager {
  /**
   * Connections carrying someone else's share to us, keyed by the sharer.
   * Kept apart from `outgoing` because both directions can exist for the same
   * person: everyone in a channel may share, and watch every other share.
   */
  private incoming: Record<string, RTCPeerConnection> = {};
  /** Connections carrying our capture to a viewer, keyed by that viewer. */
  private outgoing: Record<string, RTCPeerConnection> = {};
  /** Incoming stream per sharer, reused across renegotiations (see below). */
  private remoteStreams: Record<string, MediaStream> = {};
  private localStream: MediaStream | null = null;
  private userName: string | null = null;
  private channelId: number | null = null;
  private listeners: Array<(peers: ScreenSharePeer[]) => void> = [];
  private localStreamListeners: Array<(stream: MediaStream | null) => void> = [];
  private settings: ScreenShareSettings = DEFAULT_SETTINGS;
  /** Server-enforced bitrate cap in bits per second (null = no cap). */
  private serverMaxBitrate: number | null = null;

  /** Rebuilds spent on each share we are watching, keyed by the sharer. */
  private rebuilds: Record<string, number> = {};

  private config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

  /**
   * Repairs connections that break mid-share instead of closing them. Both
   * directions go through it, but only the viewer ever re-offers: offers are
   * the viewer's to make in this protocol, which settles who acts without the
   * tiebreak the voice manager needs.
   */
  private recovery = new PeerRecovery({
    restart: (key) => void this.restartIce(key),
    rebuild: (key) => this.rebuildConnection(key),
    setReconnecting: () => this.emit(this.getPeersList())
  });

  constructor() {
    this.setupSignaling();
  }

  private setupSignaling() {
    chat.on('screenshare-offer', (msg) => this.handleOffer(msg));
    chat.on('screenshare-answer', (msg) => this.handleAnswer(msg));
    chat.on('screenshare-candidate', (msg) => this.handleCandidate(msg));
    chat.on('screenshare-stop', (msg) => this.handleRemoteStop(msg));
  }

  subscribe(cb: (peers: ScreenSharePeer[]) => void) {
    this.listeners.push(cb);
    return () => {
      this.listeners = this.listeners.filter((fn) => fn !== cb);
    };
  }

  private emit(peers: ScreenSharePeer[]) {
    for (const cb of this.listeners) cb(peers);
  }

  /**
   * Observe the local capture stream (null while not sharing). This is the
   * only reliable signal that sharing ended, because the capture can also be
   * stopped from the browser/OS "stop sharing" bar rather than through
   * `stopSharing()`.
   */
  subscribeLocalStream(cb: (stream: MediaStream | null) => void) {
    this.localStreamListeners.push(cb);
    cb(this.localStream);
    return () => {
      this.localStreamListeners = this.localStreamListeners.filter((fn) => fn !== cb);
    };
  }

  private emitLocalStream() {
    for (const cb of this.localStreamListeners) cb(this.localStream);
  }

  /**
   * Everyone whose share we are watching, including one whose connection is
   * mid-rebuild and so has no entry in `incoming` at this instant. Anything
   * that tears watching down has to go by this rather than by the connections
   * alone, or a repair in flight survives the teardown.
   */
  private watchedSharers(): string[] {
    return [...new Set([...Object.keys(this.incoming), ...Object.keys(this.remoteStreams)])];
  }

  private getPeersList(): ScreenSharePeer[] {
    const peers: ScreenSharePeer[] = [];
    for (const [userId, pc] of Object.entries(this.incoming)) {
      // Only transceivers that actually receive. Every transceiver owns a
      // receiver with a track, including the audio one of a share that turned
      // out to be silent — going by `getReceivers()` would list that as
      // incoming media and offer volume controls for silence.
      const tracks: MediaStreamTrack[] = [];
      for (const transceiver of pc.getTransceivers()) {
        const direction = transceiver.currentDirection;
        if (direction !== 'recvonly' && direction !== 'sendrecv') continue;
        if (transceiver.receiver.track) tracks.push(transceiver.receiver.track);
      }
      if (!tracks.some((track) => track.kind === 'video')) continue;

      // The stream object is kept and updated in place rather than rebuilt:
      // handing the viewer a new `MediaStream` re-attaches the video
      // element's source, which restarts playback from black.
      let stream = this.remoteStreams[userId];
      if (!stream) {
        stream = new MediaStream();
        this.remoteStreams[userId] = stream;
      }
      for (const track of tracks) {
        if (!stream.getTrackById(track.id)) stream.addTrack(track);
      }
      for (const track of stream.getTracks()) {
        if (!tracks.includes(track)) stream.removeTrack(track);
      }

      peers.push({
        userId,
        stream,
        hasAudio: stream.getAudioTracks().length > 0,
        reconnecting: this.recovery.isRecovering(recoveryKey('viewer', userId))
      });
    }

    // A share under repair reports nothing above for a while: mid-rebuild it
    // has no connection at all, and the replacement has no video until the
    // first frame arrives. It still has to appear here — the viewer's window
    // closes the instant a share it once had media from drops out of this
    // list, and a repair is precisely when it must not. The retained stream is
    // what says the share is still ours to keep; every real teardown deletes
    // it along with the connection.
    const listed = new Set(peers.map((peer) => peer.userId));
    for (const [userId, stream] of Object.entries(this.remoteStreams)) {
      if (listed.has(userId)) continue;
      peers.push({
        userId,
        stream,
        hasAudio: stream.getAudioTracks().length > 0,
        reconnecting: true
      });
    }
    return peers;
  }

  async startSharing(
    user: string,
    channelId: number,
    settings: Partial<ScreenShareSettings> = {}
  ): Promise<void> {
    if (this.localStream) {
      throw new Error('Already sharing screen');
    }

    this.userName = user;
    this.channelId = channelId;
    this.settings = { ...DEFAULT_SETTINGS, ...settings };

    try {
      const stream = await navigator.mediaDevices.getDisplayMedia({
        video: {
          width: { ideal: this.settings.width },
          height: { ideal: this.settings.height },
          frameRate: { ideal: this.settings.frameRate }
        },
        // Asking for audio only puts the "share audio" checkbox in the OS
        // picker — the user decides, and a declined one simply yields a stream
        // without an audio track. The voice-chat processing is switched off
        // explicitly: this is game or video audio, and running it through
        // noise suppression or automatic gain control (all tuned for speech)
        // would mangle it.
        audio: {
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: false
        }
      });

      this.localStream = stream;
      this.emitLocalStream();

      // 'detail' keeps text sharp for mostly-static content; at 60 fps the
      // intent is motion (gameplay), where dropping resolution beats
      // dropping frames.
      const track = stream.getVideoTracks()[0];
      track.contentHint = this.settings.frameRate >= 60 ? 'motion' : 'detail';

      // Shared audio is music/effects, not a voice: the hint keeps the
      // encoder from applying the speech optimisations it would otherwise
      // default to on an audio track.
      for (const audioTrack of stream.getAudioTracks()) {
        audioTrack.contentHint = 'music';
      }

      chat.sendRaw({
        type: 'screenshare-start',
        user,
        channelId,
        settings: this.settings
      });

      track.addEventListener('ended', () => {
        this.stopSharing();
      });
    } catch (error) {
      this.userName = null;
      this.channelId = null;
      throw error;
    }
  }

  stopSharing(): void {
    if (!this.localStream || !this.userName || this.channelId === null) return;

    this.localStream.getTracks().forEach((track) => track.stop());
    this.localStream = null;
    this.emitLocalStream();

    chat.sendRaw({
      type: 'screenshare-stop',
      user: this.userName,
      channelId: this.channelId
    });

    // Only the connections that were carrying our capture. Shares we are
    // watching do not end because we stopped our own, so the identity and
    // channel used for their signaling have to stay as well.
    for (const userId of Object.keys(this.outgoing)) {
      this.closeOutgoing(userId);
    }

    // `remoteStreams` counts too: a share being rebuilt has no connection for
    // a moment, and dropping the identity then would leave its own repair
    // signaling with no one to send as.
    if (this.watchedSharers().length === 0) {
      this.userName = null;
      this.channelId = null;
    }
    this.emit(this.getPeersList());
  }

  updateSettings(settings: Partial<ScreenShareSettings>): void {
    this.settings = { ...this.settings, ...settings };
    this.applyBitrateLimit();
  }

  /** Update the server-enforced bitrate cap and re-apply it to live senders. */
  setServerMaxBitrate(limit: number | null): void {
    this.serverMaxBitrate = limit;
    this.applyBitrateLimit();
  }

  /** The bitrate to enforce: the user's setting capped by the server limit. */
  private effectiveMaxBitrate(): number {
    return this.serverMaxBitrate !== null
      ? Math.min(this.settings.maxBitrate, this.serverMaxBitrate)
      : this.settings.maxBitrate;
  }

  /** (Re-)apply the bitrate cap to every outgoing video sender. */
  private applyBitrateLimit(): void {
    const limit = this.effectiveMaxBitrate();
    if (!Number.isFinite(limit) || limit <= 0) return;
    for (const pc of Object.values(this.outgoing)) {
      for (const sender of pc.getSenders()) {
        if (sender.track?.kind !== 'video') continue;
        const params = sender.getParameters();
        if (!params.encodings || params.encodings.length === 0) {
          params.encodings = [{}];
        }
        for (const encoding of params.encodings) {
          encoding.maxBitrate = limit;
        }
        sender.setParameters(params).catch(() => {});
      }
    }
  }

  /**
   * ICE and failure handling, identical for both directions. `role` is our
   * own role on this connection and travels with every frame we send, so the
   * other side can tell which of the two connections it belongs to.
   */
  private wirePeer(pc: RTCPeerConnection, remote: string, role: ScreenShareRole): void {
    pc.onicecandidate = (ev) => {
      if (ev.candidate && this.userName) {
        chat.sendRaw({
          type: 'screenshare-candidate',
          user: this.userName,
          target: remote,
          channelId: this.channelId,
          role,
          candidate: ev.candidate
        });
      }
    };

    pc.onconnectionstatechange = () => {
      // `disconnected` and `failed` go to the repair controller instead of
      // ending the share. Closing on them meant a two-second Wi-Fi hiccup took
      // the viewer's window down for good, since the store treats a share that
      // vanishes from the peer list as one that ended.
      this.recovery.observe(recoveryKey(role, remote), pc.connectionState);
      if (pc.connectionState === 'closed') {
        if (role === 'viewer') this.closeIncoming(remote);
        else this.closeOutgoing(remote);
      }
    };
  }

  /**
   * Re-offer the connection over whatever path is reachable now. Only the
   * viewer does this — offers travel one way in this protocol — so the sharer
   * side simply waits for the offer that follows.
   */
  private async restartIce(key: string): Promise<void> {
    const [role, remote] = splitKey(key);
    if (role !== 'viewer') return;
    const pc = this.incoming[remote];
    if (!pc || !this.userName || pc.signalingState === 'closed') return;
    try {
      pc.restartIce();
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      chat.sendRaw({
        type: 'screenshare-offer',
        user: this.userName,
        target: remote,
        channelId: this.channelId,
        role: 'viewer',
        sdp: offer
      });
    } catch (error) {
      // Just an attempt that did not land; the controller retries and the
      // deadline still ends in a rebuild.
      console.warn('Screen share ICE restart failed:', error);
    }
  }

  /**
   * Replace a connection that ICE restarts could not save. The viewer builds a
   * new one and offers it; the sharer drops its side and lets the viewer's
   * offer recreate it. The viewer gives up after `MAX_REBUILDS` so a share
   * that is never coming back stops occupying the screen.
   */
  private rebuildConnection(key: string): void {
    const [role, remote] = splitKey(key);
    if (role === 'sharer') {
      // Nothing to rebuild from this end: without a viewer asking there is no
      // connection to make. Dropping ours frees the encode until they do.
      this.closeOutgoing(remote);
      return;
    }

    if (!this.incoming[remote]) return;
    const attempts = (this.rebuilds[remote] ?? 0) + 1;
    if (attempts > MAX_REBUILDS) {
      // Honest failure: close the share rather than leave a window that says
      // it is reconnecting and never will.
      this.closeIncoming(remote);
      return;
    }
    this.rebuilds[remote] = attempts;

    // Deliberately not `closeIncoming`: that drops the retained stream and
    // emits a peer list without this share, which is the store's signal to
    // close the window. The stream is kept and handed to the new connection.
    this.discardIncoming(remote);
    this.openIncoming(remote).catch((error) => {
      console.error('Failed to rebuild screen share connection:', error);
      // The stream is still in `remoteStreams` with nothing to fill it, which
      // would leave the window reconnecting forever. Close it properly.
      this.closeIncoming(remote);
    });
  }

  /** Close an incoming connection but keep the stream, for a rebuild. */
  private discardIncoming(sharer: string): void {
    const pc = this.incoming[sharer];
    if (!pc) return;
    this.recovery.forget(recoveryKey('viewer', sharer));
    // The handler would call `closeIncoming` on the `closed` state and take
    // the retained stream with it.
    pc.onconnectionstatechange = null;
    pc.close();
    delete this.incoming[sharer];
  }

  /** Open the connection that pulls `sharer`'s screen to us. */
  private async openIncoming(sharer: string): Promise<void> {
    if (this.incoming[sharer]) return;

    const pc = new RTCPeerConnection(this.config);
    this.incoming[sharer] = pc;

    pc.addTransceiver('video', { direction: 'recvonly' });
    // An answer can only fill m-lines the offer already contains, so without
    // asking for audio up front the sharer would have nowhere to put its
    // audio track and the shared sound would never leave the machine. Shares
    // without audio simply answer this one inactive.
    pc.addTransceiver('audio', { direction: 'recvonly' });

    pc.ontrack = () => {
      // Media arriving is the only proof a rebuild worked, so the budget for
      // further ones resets here rather than on the connection state.
      delete this.rebuilds[sharer];
      this.emit(this.getPeersList());
    };

    this.wirePeer(pc, sharer, 'viewer');

    const offer = await pc.createOffer();
    await pc.setLocalDescription(offer);
    chat.sendRaw({
      type: 'screenshare-offer',
      user: this.userName,
      target: sharer,
      channelId: this.channelId,
      role: 'viewer',
      sdp: offer
    });
  }

  /** Whether a signaling frame is addressed to us in our current channel. */
  private isForUs(msg: Message): boolean {
    return (
      !!this.userName &&
      msg.target === this.userName &&
      (msg as any).channelId === this.channelId
    );
  }

  /**
   * A viewer asking for our capture. Offers are only ever sent by viewers, so
   * this is always the outgoing direction — and pointless while we are not
   * sharing anything.
   */
  private async handleOffer(msg: Message): Promise<void> {
    if (!this.isForUs(msg) || !this.localStream) return;

    const viewer = msg.user as string;
    const offerSdp = (msg.sdp as RTCSessionDescriptionInit | undefined)?.sdp;

    // Two kinds of re-offer arrive on a connection we already have. An ICE
    // restart renegotiates this one and is answered on it; a viewer that gave
    // up and built a new connection (see `rebuildConnection`) presents a new
    // certificate, which only a new connection of ours can answer.
    const current = this.outgoing[viewer];
    if (current) {
      const rebuilt =
        current.currentRemoteDescription != null &&
        remoteFingerprint(offerSdp) !== remoteFingerprint(current.currentRemoteDescription.sdp);
      if (rebuilt || current.connectionState === 'failed' || current.signalingState === 'closed') {
        this.closeOutgoing(viewer);
      }
    }

    let pc = this.outgoing[viewer];
    if (!pc) {
      pc = new RTCPeerConnection(this.config);
      this.outgoing[viewer] = pc;
      for (const track of this.localStream.getTracks()) {
        pc.addTrack(track, this.localStream);
      }
      this.wirePeer(pc, viewer, 'sharer');
      this.applyBitrateLimit();
    }

    await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);

    chat.sendRaw({
      type: 'screenshare-answer',
      user: this.userName,
      target: viewer,
      channelId: this.channelId,
      role: 'sharer',
      sdp: answer
    });
  }

  /** Answers only ever come from a sharer, so they belong to `incoming`. */
  private async handleAnswer(msg: Message): Promise<void> {
    if (!this.isForUs(msg)) return;

    const pc = this.incoming[msg.user as string];
    // Accept an answer whenever one is outstanding, not just the first: an ICE
    // restart is a *second* round of offer/answer on a connection that already
    // has a remote description, and gating on that being absent would drop
    // every repair without a trace.
    if (pc && pc.signalingState === 'have-local-offer') {
      await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    }
  }

  /**
   * Candidates carry the sender's role, which picks the connection: what the
   * other side sends as a viewer belongs to the share we send them, and what
   * it sends as a sharer belongs to the share we receive.
   */
  private async handleCandidate(msg: Message): Promise<void> {
    if (!this.isForUs(msg)) return;

    const remote = msg.user as string;
    const pc = (msg as any).role === 'viewer' ? this.outgoing[remote] : this.incoming[remote];
    if (pc) {
      try {
        await pc.addIceCandidate(msg.candidate as any);
      } catch (error) {
        console.error('Error adding ICE candidate:', error);
      }
    }
  }

  private handleRemoteStop(msg: Message): void {
    // Their share ended, so the stream we were pulling from them is gone. Any
    // connection in the other direction is theirs to end, not ours.
    if ((msg as any).channelId === this.channelId) {
      this.closeIncoming(msg.user as string);
    }
  }

  private closeIncoming(sharer: string): void {
    // Also runs for a share being rebuilt, whose connection is already gone:
    // the retained stream and the repair timers have to go either way, or the
    // share stays listed as reconnecting with nothing behind it.
    this.recovery.forget(recoveryKey('viewer', sharer));
    delete this.remoteStreams[sharer];
    delete this.rebuilds[sharer];
    const pc = this.incoming[sharer];
    if (pc) {
      pc.close();
      delete this.incoming[sharer];
    }
    this.emit(this.getPeersList());
  }

  private closeOutgoing(viewer: string): void {
    this.recovery.forget(recoveryKey('sharer', viewer));
    const pc = this.outgoing[viewer];
    if (!pc) return;
    pc.close();
    delete this.outgoing[viewer];
  }

  async viewScreenShare(userId: string, viewerName?: string, channelId?: number): Promise<void> {
    if (viewerName && channelId !== undefined && !this.userName && this.channelId === null) {
      this.userName = viewerName;
      this.channelId = channelId;
    }

    if (!this.userName || this.channelId === null) {
      throw new Error('Not in a voice channel');
    }

    await this.openIncoming(userId);
  }

  stopViewing(userId: string): void {
    this.closeIncoming(userId);
  }

  leaveAsViewer(): void {
    if (!this.isSharing()) {
      for (const userId of this.watchedSharers()) {
        this.closeIncoming(userId);
      }
      this.userName = null;
      this.channelId = null;
      this.emit([]);
    }
  }

  isSharing(): boolean {
    return this.localStream !== null;
  }

  getSettings(): ScreenShareSettings {
    return { ...this.settings };
  }

  destroy(): void {
    this.stopSharing();
    for (const userId of this.watchedSharers()) {
      this.closeIncoming(userId);
    }
    chat.off('screenshare-offer');
    chat.off('screenshare-answer');
    chat.off('screenshare-candidate');
    chat.off('screenshare-stop');
  }
}
