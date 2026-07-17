/**
 * WebRTC voice chat manager.
 *
 * Handles peer connection setup using messages sent over the WebSocket chat
 * channel. Consumers subscribe to updates to receive the list of remote peers
 * currently connected.
 */
import { chat } from '../stores/chat';
import {
  volume,
  inputDeviceId,
  microphoneMuted,
  outputMuted,
  voiceMode,
  vadSensitivity,
  pttKey,
  isPttActive,
  voiceActivity,
  echoCancellation,
  noiseSuppression,
  autoGainControl
} from '../stores/settings';
import { resetRemoteSpeaking } from '../stores/voiceSpeaking';
import { get } from 'svelte/store';
import type { Message, RemotePeer, ConnectionStats, VoiceChannelInfo } from '../types';
import { VoiceActivityDetector } from './vad';
import { PushToTalkManager } from './ptt';

const DEFAULT_AUDIO_BITRATE = 64_000;

export class VoiceManager {
  private peers: Record<string, RTCPeerConnection> = {};
  private statsIntervals: Record<string, number> = {};
  /** Cumulative RTP packet counters per peer, used to compute windowed loss. */
  private prevPacketCounts: Record<
    string,
    { received: number; lost: number; sent: number; remoteLost: number }
  > = {};
  private localStream: MediaStream | null = null;
  private userName: string | null = null;
  private channelId: number | null = null;
  private listeners: Array<(peers: RemotePeer[]) => void> = [];

  private vad: VoiceActivityDetector | null = null;
  private ptt: PushToTalkManager | null = null;
  private shouldTransmit = false;

  private audioContext: AudioContext | null = null;
  private gainNode: GainNode | null = null;
  private sourceNode: MediaStreamAudioSourceNode | null = null;
  private destinationStream: MediaStream | null = null;
  private rawStream: MediaStream | null = null;

  private joinSound = new Audio('/sounds/user_join_voice_sound.mp3');
  private leaveSound = new Audio('/sounds/user_leave_voice_sound.mp3');
  private muteSound = new Audio('/sounds/mute_sound.wav');
  private unmuteSound = new Audio('/sounds/unmute_sound.wav');

  private config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

  private channelConfig: VoiceChannelInfo | null = null;

  constructor() {
    volume.subscribe((v) => {
      this.joinSound.volume = v;
      this.leaveSound.volume = v;
      this.muteSound.volume = v;
      this.unmuteSound.volume = v;
    });

    // Skip the first (synchronous) subscribe call so the persisted mute state
    // restored on startup doesn't trigger a blip.
    let micInitialized = false;
    microphoneMuted.subscribe((muted) => {
      this.updateTransmissionState();
      this.broadcastMuteState();
      if (!micInitialized) {
        micInitialized = true;
        return;
      }
      this.playMuteSound(muted);
    });

    let outputInitialized = false;
    outputMuted.subscribe((muted) => {
      this.broadcastMuteState();
      if (!outputInitialized) {
        outputInitialized = true;
        return;
      }
      this.playMuteSound(muted);
    });

    this.vad = new VoiceActivityDetector();
    this.ptt = new PushToTalkManager(get(pttKey));

    voiceMode.subscribe(() => this.updateTransmissionMode());
    vadSensitivity.subscribe(() => this.updateVadSensitivity());
    echoCancellation.subscribe(() => this.applyMicProcessing());
    noiseSuppression.subscribe(() => this.applyMicProcessing());
    autoGainControl.subscribe(() => this.applyMicProcessing());
    pttKey.subscribe((key) => {
      if (this.ptt) {
        this.ptt.setKey(key);
      }
    });

    this.vad.subscribe((isActive, level) => {
      voiceActivity.set(isActive);
      this.updateTransmissionState();
    });

    this.ptt.subscribe((isPressed) => {
      isPttActive.set(isPressed);
      this.updateTransmissionState();
    });

    chat.on('voice-channel-update', (msg) => {
      const chId = (msg as any).channelId;
      if (typeof chId !== 'number' || this.channelId !== chId) return;
      const quality =
        typeof (msg as any).quality === 'string' && (msg as any).quality.trim()
          ? (msg as any).quality.trim()
          : (this.channelConfig?.quality ?? 'standard');
      let bitrate: number | null = this.channelConfig?.bitrate ?? DEFAULT_AUDIO_BITRATE;
      if ((msg as any).bitrate === null) {
        bitrate = null;
      } else if (typeof (msg as any).bitrate === 'number' && Number.isFinite((msg as any).bitrate)) {
        bitrate = Math.max(0, Math.round((msg as any).bitrate));
      }
      this.channelConfig = {
        id: chId,
        name: this.channelConfig?.name ?? '',
        quality,
        bitrate,
        categoryId: this.channelConfig?.categoryId ?? null,
        position: this.channelConfig?.position ?? 0
      };
      this.applyChannelConfigToPeers();
    });
  }

  private updateTransmissionMode(rawStream?: MediaStream) {
    const streamToUse = rawStream || this.getVadStream();
    if (!streamToUse) return;

    const mode = get(voiceMode);

    if (mode === 'vad' && this.vad) {
      this.vad.start(streamToUse, get(vadSensitivity));
    } else if (this.vad) {
      this.vad.stop();
    }

    this.updateTransmissionState();
  }

  private getVadStream(): MediaStream | null {
    return this.rawStream;
  }

  private updateVadSensitivity() {
    if (get(voiceMode) === 'vad' && this.vad) {
      this.vad.updateSensitivity(get(vadSensitivity));
    }
  }

  private micProcessingConstraints(): MediaTrackConstraints {
    return {
      echoCancellation: get(echoCancellation),
      noiseSuppression: get(noiseSuppression),
      autoGainControl: get(autoGainControl)
    };
  }

  /** Re-apply the mic processing settings to the live capture track. */
  private applyMicProcessing() {
    const track = this.rawStream?.getAudioTracks()[0];
    if (!track) return;
    track.applyConstraints(this.micProcessingConstraints()).catch((error) => {
      console.warn('Failed to apply microphone processing constraints:', error);
    });
  }

  /**
   * Tell the other clients in the channel whether our microphone and/or
   * speaker are muted so they can show an indicator beside our name. No-op
   * while not connected to a voice channel.
   */
  private broadcastMuteState() {
    if (!this.userName || this.channelId === null) return;
    chat.sendRaw({
      type: 'voice-mute',
      user: this.userName,
      channelId: this.channelId,
      micMuted: get(microphoneMuted),
      outputMuted: get(outputMuted)
    });
  }

  /** Play the short mute/unmute feedback blip. */
  private playMuteSound(muted: boolean) {
    const sound = muted ? this.muteSound : this.unmuteSound;
    try {
      sound.currentTime = 0;
      sound.play().catch(() => {});
    } catch {}
  }

  private updateTransmissionState() {
    if (!this.localStream) return;

    const mode = get(voiceMode);
    const isMuted = get(microphoneMuted);
    let shouldTransmit = false;

    if (!isMuted) {
      switch (mode) {
        case 'continuous':
          shouldTransmit = true;
          break;
        case 'vad':
          shouldTransmit = get(voiceActivity);
          break;
        case 'ptt':
          shouldTransmit = get(isPttActive);
          break;
      }
    }

    this.shouldTransmit = shouldTransmit;
    this.applyTransmissionState();
  }

  private applyTransmissionState() {
    if (!this.gainNode) return;
    const shouldTransmitAudio = this.shouldTransmit && !get(microphoneMuted);
    this.gainNode.gain.value = shouldTransmitAudio ? 1.0 : 0.0;
  }

  private configureSender(sender: RTCRtpSender) {
    if (!this.channelConfig) return;
    try {
      const params = sender.getParameters();
      if (!params.encodings || params.encodings.length === 0) {
        params.encodings = [{}];
      }
      const target = this.channelConfig.bitrate;
      for (const encoding of params.encodings) {
        if (target && target > 0) {
          encoding.maxBitrate = target;
        } else {
          delete (encoding as any).maxBitrate;
        }
      }
      sender.setParameters(params).catch(() => {});
    } catch {
      // Ignore configuration errors
    }
  }

  private applyChannelConfigToPeers() {
    if (!this.channelConfig) return;
    for (const pc of Object.values(this.peers)) {
      for (const sender of pc.getSenders()) {
        if (sender.track && sender.track.kind === 'audio') {
          this.configureSender(sender);
        }
      }
    }
  }

  private async setupAudioProcessing(inputStream: MediaStream): Promise<MediaStream> {
    try {
      this.rawStream = inputStream;
      this.audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
      this.sourceNode = this.audioContext.createMediaStreamSource(inputStream);
      this.gainNode = this.audioContext.createGain();
      this.gainNode.gain.value = 1.0;
      const destination = this.audioContext.createMediaStreamDestination();
      this.sourceNode.connect(this.gainNode);
      this.gainNode.connect(destination);
      this.destinationStream = destination.stream;
      return this.destinationStream;
    } catch (error) {
      console.error('Failed to setup audio processing:', error);
      return inputStream;
    }
  }

  private cleanupAudioProcessing() {
    if (this.sourceNode) {
      this.sourceNode.disconnect();
      this.sourceNode = null;
    }
    if (this.gainNode) {
      this.gainNode.disconnect();
      this.gainNode = null;
    }
    if (this.audioContext && this.audioContext.state !== 'closed') {
      this.audioContext.close();
      this.audioContext = null;
    }
    if (this.rawStream) {
      for (const track of this.rawStream.getTracks()) {
        track.stop();
      }
      this.rawStream = null;
    }
    this.destinationStream = null;
  }

  subscribe(cb: (peers: RemotePeer[]) => void) {
    this.listeners.push(cb);
    return () => {
      this.listeners = this.listeners.filter((fn) => fn !== cb);
    };
  }

  private emit(peers: RemotePeer[]) {
    for (const cb of this.listeners) cb(peers);
  }

  private cleanupPeer(id: string, peersList: RemotePeer[]) {
    const pc = this.peers[id];
    if (pc) {
      pc.close();
      delete this.peers[id];
      const interval = this.statsIntervals[id];
      if (interval) {
        clearInterval(interval);
        delete this.statsIntervals[id];
      }
      delete this.prevPacketCounts[id];
    }
    this.emit(peersList.filter((r) => r.id !== id));
  }

  private async updateStats(id: string, peersList: RemotePeer[]) {
    const pc = this.peers[id];
    if (!pc) return;
    try {
      const reports = await pc.getStats();
      let rtt = 0;
      let jitter = 0;
      let received = 0;
      let lost = 0;
      let sent = 0;
      let remoteLost = 0;
      reports.forEach((report) => {
        if (
          report.type === 'candidate-pair' &&
          (report as any).state === 'succeeded' &&
          (report as any).currentRoundTripTime != null
        ) {
          rtt = (report as any).currentRoundTripTime * 1000;
        }
        if (report.type === 'remote-inbound-rtp' && (report as any).kind === 'audio') {
          if ((report as any).jitter != null) {
            jitter = (report as any).jitter * 1000;
          }
          remoteLost += (report as any).packetsLost ?? 0;
        }
        if (report.type === 'inbound-rtp' && (report as any).kind === 'audio') {
          received += (report as any).packetsReceived ?? 0;
          lost += (report as any).packetsLost ?? 0;
        }
        if (report.type === 'outbound-rtp' && (report as any).kind === 'audio') {
          sent += (report as any).packetsSent ?? 0;
        }
      });

      // Loss over the window since the previous poll, in both directions:
      // packets we didn't receive plus packets the peer reports missing from
      // us. Deltas are clamped because the cumulative counters may decrease
      // (e.g. after duplicate packets or an SSRC restart).
      const prev = this.prevPacketCounts[id] ?? { received: 0, lost: 0, sent: 0, remoteLost: 0 };
      this.prevPacketCounts[id] = { received, lost, sent, remoteLost };
      const dReceived = Math.max(0, received - prev.received);
      const dLost = Math.max(0, lost - prev.lost);
      const dSent = Math.max(0, sent - prev.sent);
      const dRemoteLost = Math.max(0, remoteLost - prev.remoteLost);
      const inboundLoss = dReceived + dLost > 0 ? (dLost / (dReceived + dLost)) * 100 : 0;
      const outboundLoss = dSent > 0 ? Math.min(100, (dRemoteLost / dSent) * 100) : 0;
      const packetLoss = Math.max(inboundLoss, outboundLoss);

      let strength =
        rtt === 0 ? 5 : rtt < 50 ? 5 : rtt < 100 ? 4 : rtt < 200 ? 3 : rtt < 400 ? 2 : 1;
      // Heavy packet loss ruins a call even on a fast link, so cap the bars.
      if (packetLoss >= 10) strength = Math.min(strength, 1);
      else if (packetLoss >= 5) strength = Math.min(strength, 2);
      else if (packetLoss >= 2) strength = Math.min(strength, 3);

      for (const p of peersList) {
        if (p.id === id) {
          p.stats = { rtt, jitter, packetLoss, strength };
        }
      }
      this.emit([...peersList]);
    } catch {
      // ignore stats errors
    }
  }

  private async createPeer(
    id: string,
    initiator: boolean,
    peersList: RemotePeer[]
  ): Promise<RTCPeerConnection> {
    if (this.peers[id]) return this.peers[id];
    const pc = new RTCPeerConnection(this.config);
    this.peers[id] = pc;
    if (this.localStream) {
      for (const track of this.localStream.getTracks()) {
        const sender = pc.addTrack(track, this.localStream);
        if (track.kind === 'audio') {
          this.configureSender(sender);
        }
      }
    }
    pc.ontrack = (ev) => {
      const stream = ev.streams[0];
      const existing = peersList.find((r) => r.id === id);
      if (existing) {
        existing.stream = stream;
      } else {
        peersList.push({ id, stream });
      }
      this.emit([...peersList]);
    };
    pc.onicecandidate = (ev) => {
      if (ev.candidate && this.userName) {
        chat.sendRaw({
          type: 'voice-candidate',
          user: this.userName,
          target: id,
          channelId: this.channelId,
          candidate: ev.candidate
        });
      }
    };
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'disconnected' || pc.connectionState === 'closed') {
        this.cleanupPeer(id, peersList);
      }
    };
    this.statsIntervals[id] = window.setInterval(() => this.updateStats(id, peersList), 1000);
    if (initiator && this.userName) {
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      chat.sendRaw({
        type: 'voice-offer',
        user: this.userName,
        target: id,
        channelId: this.channelId,
        sdp: offer
      });
    }
    return pc;
  }

  /**
   * Join a voice channel and start streaming the local microphone.
   *
   * Rejects (without changing any state) when microphone access fails, so a
   * denied permission prompt doesn't leave the manager stuck in a half-joined
   * state that blocks all future joins.
   */
  async join(user: string, channelId: number, peersList: RemotePeer[], info?: VoiceChannelInfo) {
    if (this.userName) return;

    // Acquire the microphone before touching any state: this is the only
    // step that can fail.
    const device = get(inputDeviceId);
    const audio: MediaTrackConstraints = this.micProcessingConstraints();
    if (device) audio.deviceId = { exact: device };
    const rawStream = await navigator.mediaDevices.getUserMedia({ audio });

    this.userName = user;
    this.channelId = channelId;
    this.channelConfig = info
      ? {
          id: channelId,
          name: info.name,
          quality: info.quality,
          bitrate: info.bitrate,
          categoryId: info.categoryId ?? null,
          position: info.position ?? 0
        }
      : {
          id: channelId,
          name: '',
          quality: 'standard',
          bitrate: DEFAULT_AUDIO_BITRATE,
          categoryId: null,
          position: 0
        };
    resetRemoteSpeaking();
    chat.on('voice-join', (m) => this.handleJoin(m, peersList));
    chat.on('voice-offer', (m) => this.handleOffer(m, peersList));
    chat.on('voice-answer', (m) => this.handleAnswer(m));
    chat.on('voice-candidate', (m) => this.handleCandidate(m));
    chat.on('voice-leave', (m) => this.handleLeave(m, peersList));

    this.localStream = await this.setupAudioProcessing(rawStream);
    this.updateTransmissionMode(rawStream);

    chat.sendRaw({ type: 'voice-join', user, channelId });
    this.broadcastMuteState();
  }

  /**
   * Leave the current voice channel and clean up all peer connections.
   */
  leave(channelId: number, peersList: RemotePeer[]) {
    if (!this.userName) return;
    chat.sendRaw({ type: 'voice-leave', user: this.userName, channelId });
    for (const id of Object.keys(this.peers)) {
      this.cleanupPeer(id, peersList);
    }
    this.cleanupAudioProcessing();
    this.localStream = null;

    if (this.vad) {
      this.vad.stop();
    }

    voiceActivity.set(false);
    isPttActive.set(false);

    chat.off('voice-join');
    chat.off('voice-offer');
    chat.off('voice-answer');
    chat.off('voice-candidate');
    chat.off('voice-leave');
    this.userName = null;
    this.channelId = null;
    this.channelConfig = null;
    peersList.length = 0;
    this.emit([]);
    resetRemoteSpeaking();
  }

  private handleJoin(msg: Message, peersList: RemotePeer[]) {
    if (
      !this.userName ||
      msg.user === this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;
    this.createPeer(msg.user as string, true, peersList);
    try {
      this.joinSound.currentTime = 0;
      this.joinSound.play();
    } catch {}
  }

  private async handleOffer(msg: Message, peersList: RemotePeer[]) {
    if (
      !this.userName ||
      msg.target !== this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;
    const pc = await this.createPeer(msg.user as string, false, peersList);
    await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);
    chat.sendRaw({
      type: 'voice-answer',
      user: this.userName,
      target: msg.user,
      channelId: this.channelId,
      sdp: answer
    });
  }

  private async handleAnswer(msg: Message) {
    if (
      !this.userName ||
      msg.target !== this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;
    const pc = this.peers[msg.user as string];
    if (pc && !pc.currentRemoteDescription) {
      await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    }
  }

  private async handleCandidate(msg: Message) {
    if (
      !this.userName ||
      msg.target !== this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;
    const pc = this.peers[msg.user as string];
    if (pc) {
      try {
        await pc.addIceCandidate(msg.candidate as any);
      } catch {}
    }
  }

  private handleLeave(msg: Message, peersList: RemotePeer[]) {
    if (!this.userName || (msg as any).channelId !== this.channelId) return;
    this.cleanupPeer(msg.user as string, peersList);
    try {
      this.leaveSound.currentTime = 0;
      this.leaveSound.play();
    } catch {}
  }

  destroy() {
    if (this.vad) {
      this.vad.destroy();
      this.vad = null;
    }

    if (this.ptt) {
      this.ptt.destroy();
      this.ptt = null;
    }

    resetRemoteSpeaking();
  }
}
