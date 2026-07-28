/**
 * WebRTC screen sharing manager.
 *
 * Handles peer-to-peer screen sharing connections using WebRTC. Allows users
 * to share their screen with configurable quality and frame rate settings.
 * Uses the same signaling infrastructure as voice chat.
 */
import { chat } from '../stores/chat';
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

  private config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

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

      peers.push({ userId, stream, hasAudio: stream.getAudioTracks().length > 0 });
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

    if (Object.keys(this.incoming).length === 0) {
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
      if (pc.connectionState === 'disconnected' || pc.connectionState === 'failed') {
        if (role === 'viewer') this.closeIncoming(remote);
        else this.closeOutgoing(remote);
      }
    };
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
    if (pc && !pc.currentRemoteDescription) {
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
    const pc = this.incoming[sharer];
    if (!pc) return;
    pc.close();
    delete this.incoming[sharer];
    delete this.remoteStreams[sharer];
    this.emit(this.getPeersList());
  }

  private closeOutgoing(viewer: string): void {
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
      for (const userId of Object.keys(this.incoming)) {
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
    for (const userId of Object.keys(this.incoming)) {
      this.closeIncoming(userId);
    }
    chat.off('screenshare-offer');
    chat.off('screenshare-answer');
    chat.off('screenshare-candidate');
    chat.off('screenshare-stop');
  }
}
