/**
 * WebRTC voice chat manager.
 *
 * Handles peer connection setup using messages sent over the WebSocket chat
 * channel. Consumers subscribe to updates to receive the list of remote peers
 * currently connected.
 */
import { chat } from '../stores/chat';
import {
  appSoundVolume,
  inputDeviceId,
  outputDeviceId,
  microphoneMuted,
  outputMuted,
  voiceMode,
  vadSensitivity,
  vadAutoSensitivity,
  pttKey,
  isPttActive,
  voiceActivity,
  echoCancellation,
  noiseSuppressionMode,
  autoGainControl,
  micGain,
  clampMicGain
} from '../stores/settings';
import { resetSpeaking, setSpeaking, SPEAKING_RMS_THRESHOLD } from '../stores/voiceSpeaking';
import { captureStream } from '../stores/voiceCapture';
import { get } from 'svelte/store';
import type { Message, RemotePeer, ConnectionStats, VoiceChannelInfo } from '../types';
import { VoiceActivityDetector } from './vad';
import { PushToTalkManager } from './ptt';
import { getAudioContext, resumeAudioContext } from './audioContext';
import { subscribeTick } from './ticker';
import { micProcessingConstraints, openMicrophone } from './capture';
import { connectMicSource, type MicSource } from './denoise';
import { withOpusFeatures } from './sdp';

const DEFAULT_AUDIO_BITRATE = 64_000;

/**
 * Time constant for opening and closing the transmission gate. Switching the
 * gain between 0 and 1 in a single sample produces an audible click at the
 * start and end of every voice-activated or push-to-talk burst; ramping over
 * a few milliseconds is inaudible and removes it.
 */
const GATE_RAMP_SECONDS = 0.015;

/**
 * Time constant for input gain changes. Dragging the slider steps the value
 * many times a second; applying each step instantly produces zipper noise, so
 * every change is ramped over a few dozen milliseconds instead.
 */
const INPUT_GAIN_RAMP_SECONDS = 0.05;

/**
 * Packets a loss window must hold before its percentage is recomputed. At the
 * 50 packets/s of an active stream this is a fraction of a second; on a stream
 * that DTX has reduced to comfort noise it spans several polls, which is the
 * point — see `updateStats`.
 */
const MIN_LOSS_SAMPLE_PACKETS = 20;

export class VoiceManager {
  private peers: Record<string, RTCPeerConnection> = {};
  private statsIntervals: Record<string, number> = {};
  /** Cumulative RTP packet counters per peer, used to compute windowed loss. */
  private prevPacketCounts: Record<
    string,
    { received: number; lost: number; sent: number; remoteLost: number }
  > = {};
  /**
   * Packets counted towards the loss percentage currently on screen, per peer.
   * The counters reset every time a percentage is computed; `inbound`/
   * `outbound` hold the last result so the bars have a value to show while the
   * next window fills.
   */
  private lossWindows: Record<
    string,
    {
      received: number;
      lost: number;
      sent: number;
      remoteLost: number;
      inbound: number;
      outbound: number;
    }
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
  /**
   * Input volume, applied ahead of the gate so voice detection and the level
   * meters see the same signal the peers receive.
   */
  private inputGainNode: GainNode | null = null;
  /** Microphone end of the chain, including the RNNoise node when enabled. */
  private micSource: MicSource | null = null;
  private destinationStream: MediaStream | null = null;
  private rawStream: MediaStream | null = null;

  /** Level meter on the outgoing audio, driving our own talking indicator. */
  private levelAnalyser: AnalyserNode | null = null;
  private levelBuffer: Uint8Array<ArrayBuffer> | null = null;
  private stopLevelTicks: (() => void) | null = null;

  /**
   * Serialises microphone re-acquisitions. Toggling several processing
   * settings in a row (or switching device mid-swap) would otherwise leave
   * two captures racing to own `rawStream`, and the loser's device stays open.
   */
  private captureSwap: Promise<void> = Promise.resolve();

  private joinSound = new Audio('/sounds/user_join_voice_sound.mp3');
  private leaveSound = new Audio('/sounds/user_leave_voice_sound.mp3');
  private muteSound = new Audio('/sounds/mute_sound.wav');
  private unmuteSound = new Audio('/sounds/unmute_sound.wav');

  private config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

  private channelConfig: VoiceChannelInfo | null = null;

  constructor() {
    appSoundVolume.subscribe((v) => {
      for (const sound of this.notificationSounds()) {
        sound.volume = v;
      }
    });

    // These are plain elements rather than nodes on the shared context, so
    // they need the output device applied individually — without this they
    // always played on the system default while voice went to the headset.
    outputDeviceId.subscribe((id) => {
      for (const sound of this.notificationSounds()) {
        const element = sound as HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> };
        element.setSinkId?.(id || '').catch((error: unknown) => {
          console.warn('Failed to route notification sounds to the selected output:', error);
        });
      }
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

    voiceMode.subscribe(() => {
      this.updateTransmissionMode();
      this.syncGlobalPushToTalk();
    });
    vadSensitivity.subscribe(() => this.updateVadSensitivity());
    vadAutoSensitivity.subscribe(() => this.updateVadSensitivity());
    echoCancellation.subscribe(() => this.applyMicProcessing());
    autoGainControl.subscribe(() => this.applyMicProcessing());
    // Unlike the other two this changes the shape of the graph and not just
    // the capture constraints — RNNoise is a node in the chain — so it always
    // takes the full rebuild rather than trying `applyConstraints` first.
    noiseSuppressionMode.subscribe(() => this.swapCapture());
    micGain.subscribe(() => this.applyInputGain());
    // The device used to be read once at join time, so picking a different
    // microphone mid-call did nothing until you left and rejoined.
    inputDeviceId.subscribe(() => this.swapCapture());
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

  private updateTransmissionMode() {
    const source = this.inputGainNode;
    if (!source) return;

    const mode = get(voiceMode);

    if (mode === 'vad' && this.vad) {
      this.vad.start(source, get(vadSensitivity), get(vadAutoSensitivity));
    } else if (this.vad) {
      this.vad.stop();
    }

    this.updateTransmissionState();
  }

  /**
   * Hold the OS-level push-to-talk grab only while it can actually do
   * something: in a channel, in push-to-talk mode.
   */
  private syncGlobalPushToTalk() {
    this.ptt?.setGlobalEnabled(this.userName !== null && get(voiceMode) === 'ptt');
  }

  private updateVadSensitivity() {
    if (get(voiceMode) === 'vad' && this.vad) {
      this.vad.updateSensitivity(get(vadSensitivity), get(vadAutoSensitivity));
    }
  }

  /**
   * Recover from the microphone being unplugged mid-call: the track ends and
   * the capture goes permanently silent, which used to leave the user
   * apparently connected but inaudible until they rejoined.
   */
  private watchCaptureTrack(stream: MediaStream) {
    const track = stream.getAudioTracks()[0];
    if (!track) return;
    track.addEventListener('ended', () => {
      // Only react while this is still the live capture.
      if (this.rawStream !== stream || !this.userName) return;
      console.warn('Microphone capture ended, reopening');
      this.swapCapture();
    });
  }

  /**
   * Re-apply the mic processing settings to the live capture track.
   *
   * `applyConstraints` is the cheap path, but not every platform can
   * reconfigure audio processing on a running track (WebKit rejects), and a
   * rejection there left the settings silently doing nothing despite the UI
   * promising they apply immediately. Re-opening the microphone always works.
   */
  private applyMicProcessing() {
    const track = this.rawStream?.getAudioTracks()[0];
    if (!track) return;
    track.applyConstraints(micProcessingConstraints()).catch(() => {
      this.swapCapture();
    });
  }

  /**
   * Replace the live capture with a freshly opened one. The outgoing track
   * belongs to the gain node's destination, not to the microphone, so the
   * peer connections are untouched — nobody hears a gap and no renegotiation
   * is needed.
   */
  private swapCapture() {
    if (!this.userName) return; // Settings apply at the next join.
    this.captureSwap = this.captureSwap
      .then(async () => {
        if (!this.userName || !this.inputGainNode) return;
        const context = getAudioContext();
        if (!context) return;

        const stream = await openMicrophone();
        // The session may have ended while the device was opening.
        if (!this.userName || !this.inputGainNode) {
          for (const track of stream.getTracks()) track.stop();
          return;
        }

        if (this.micSource) {
          this.micSource.disconnect();
          this.micSource = null;
        }
        if (this.rawStream) {
          for (const track of this.rawStream.getTracks()) track.stop();
        }

        const micSource = await connectMicSource(context, stream, this.inputGainNode);
        // Building the chain may have to load RNNoise first, so check again.
        if (!this.userName || !this.inputGainNode) {
          micSource.disconnect();
          for (const track of stream.getTracks()) track.stop();
          return;
        }

        this.rawStream = stream;
        this.micSource = micSource;
        this.watchCaptureTrack(stream);
        captureStream.set(stream);
        // The detector listens on the input gain node, which the swap leaves
        // in place, so it keeps running across the device change untouched.
      })
      .catch((error) => {
        console.error('Failed to switch the microphone:', error);
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

  private notificationSounds(): HTMLAudioElement[] {
    return [this.joinSound, this.leaveSound, this.muteSound, this.unmuteSound];
  }

  private playSound(sound: HTMLAudioElement) {
    try {
      sound.currentTime = 0;
      sound.play().catch(() => {});
    } catch {}
  }

  /**
   * Play the short mute/unmute feedback blip. Deliberately audible even while
   * deafened: it is confirmation of the key you just pressed, which is the
   * one thing you still need to hear when everything else is silenced — and
   * the hotkey usually gets pressed with the window out of sight.
   */
  private playMuteSound(muted: boolean) {
    this.playSound(muted ? this.muteSound : this.unmuteSound);
  }

  /**
   * Play a sound caused by somebody else. Deafening means "I hear nothing
   * from this channel", so these are suppressed along with the voice itself.
   */
  private playPeerSound(sound: HTMLAudioElement) {
    if (get(outputMuted)) return;
    this.playSound(sound);
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

  /** Push the configured input volume onto the live graph. */
  private applyInputGain() {
    if (!this.inputGainNode) return;
    const target = clampMicGain(get(micGain));
    const gain = this.inputGainNode.gain;
    const now = this.inputGainNode.context.currentTime;
    try {
      gain.cancelScheduledValues(now);
      gain.setValueAtTime(gain.value, now);
      gain.linearRampToValueAtTime(target, now + INPUT_GAIN_RAMP_SECONDS);
    } catch {
      gain.value = target;
    }
  }

  private applyTransmissionState() {
    if (!this.gainNode) return;
    const shouldTransmitAudio = this.shouldTransmit && !get(microphoneMuted);
    const target = shouldTransmitAudio ? 1.0 : 0.0;
    const gain = this.gainNode.gain;
    const now = this.gainNode.context.currentTime;
    // Ramp instead of stepping: an instant gain change clicks on every open
    // and close of the gate. The ramp is linear rather than exponential
    // because closing the gate has to reach exactly zero — an exponential
    // approach would leave the microphone very quietly open forever.
    try {
      gain.cancelScheduledValues(now);
      gain.setValueAtTime(gain.value, now);
      gain.linearRampToValueAtTime(target, now + GATE_RAMP_SECONDS);
    } catch {
      gain.value = target;
    }
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

  /**
   * Build microphone -> [noise suppression] -> input volume -> gate ->
   * outgoing-track graph.
   *
   * The two gain nodes are kept separate on purpose even though multiplying
   * them would send identical audio: the detector and the level meters tap
   * the input node, so they measure the amplified signal while the gate
   * stays a plain open/closed switch. Noise suppression goes in ahead of both
   * (see `denoise.ts`), so everything measured downstream is measured on the
   * cleaned-up signal.
   *
   * Throws when the graph cannot be built. The caller aborts the join in that
   * case rather than falling back to sending the raw capture: without the
   * gain node there is no gate, and push-to-talk or voice-activity mode would
   * quietly become an always-open microphone.
   */
  private async setupAudioProcessing(inputStream: MediaStream): Promise<MediaStream> {
    this.rawStream = inputStream;
    this.audioContext = getAudioContext();
    if (!this.audioContext) {
      throw new Error('Audio processing is unavailable on this system');
    }
    resumeAudioContext();
    try {
      this.inputGainNode = this.audioContext.createGain();
      this.inputGainNode.gain.value = clampMicGain(get(micGain));
      this.gainNode = this.audioContext.createGain();
      // Start closed and let `updateTransmissionState` open it: joining in
      // push-to-talk or voice-activity mode must not leak the first instants
      // of audio before the first gate evaluation.
      this.gainNode.gain.value = 0;
      const destination = this.audioContext.createMediaStreamDestination();
      this.inputGainNode.connect(this.gainNode);
      this.gainNode.connect(destination);
      this.micSource = await connectMicSource(this.audioContext, inputStream, this.inputGainNode);
      this.destinationStream = destination.stream;
      this.startLocalLevelMeter();
      return this.destinationStream;
    } catch (error) {
      console.error('Failed to set up audio processing:', error);
      this.cleanupAudioProcessing();
      throw error;
    }
  }

  /**
   * Meter the audio *behind* the transmission gate so our own talking
   * indicator matches what the other members actually hear: the gain node is
   * silenced whenever we are muted or not transmitting, which makes this work
   * the same for "Always On", voice detection and push-to-talk.
   */
  private startLocalLevelMeter() {
    if (!this.audioContext || !this.gainNode) return;
    this.stopLocalLevelMeter();

    const analyser = this.audioContext.createAnalyser();
    analyser.fftSize = 512;
    this.gainNode.connect(analyser);
    this.levelAnalyser = analyser;
    this.levelBuffer = new Uint8Array(new ArrayBuffer(analyser.fftSize)) as Uint8Array<ArrayBuffer>;

    this.stopLevelTicks = subscribeTick(() => {
      if (!this.levelAnalyser || !this.levelBuffer || !this.userName) return;
      this.levelAnalyser.getByteTimeDomainData(this.levelBuffer);
      let sum = 0;
      for (let i = 0; i < this.levelBuffer.length; i++) {
        const value = (this.levelBuffer[i] - 128) / 128;
        sum += value * value;
      }
      const rms = Math.sqrt(sum / this.levelBuffer.length);
      setSpeaking(this.userName, rms > SPEAKING_RMS_THRESHOLD);
    });
  }

  private stopLocalLevelMeter() {
    if (this.stopLevelTicks) {
      this.stopLevelTicks();
      this.stopLevelTicks = null;
    }
    if (this.levelAnalyser) {
      this.levelAnalyser.disconnect();
      this.levelAnalyser = null;
    }
    this.levelBuffer = null;
  }

  private cleanupAudioProcessing() {
    this.stopLocalLevelMeter();
    if (this.micSource) {
      this.micSource.disconnect();
      this.micSource = null;
    }
    if (this.inputGainNode) {
      this.inputGainNode.disconnect();
      this.inputGainNode = null;
    }
    if (this.gainNode) {
      this.gainNode.disconnect();
      this.gainNode = null;
    }
    // The context is shared app-wide and outlives the session — only our own
    // nodes are torn down.
    this.audioContext = null;
    if (this.rawStream) {
      for (const track of this.rawStream.getTracks()) {
        track.stop();
      }
      this.rawStream = null;
    }
    captureStream.set(null);
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
      delete this.lossWindows[id];
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

      // Loss in both directions: packets we didn't receive plus packets the
      // peer reports missing from us. Deltas are clamped because the
      // cumulative counters may decrease (e.g. after duplicate packets or an
      // SSRC restart).
      const prev = this.prevPacketCounts[id] ?? { received: 0, lost: 0, sent: 0, remoteLost: 0 };
      this.prevPacketCounts[id] = { received, lost, sent, remoteLost };
      const dReceived = Math.max(0, received - prev.received);
      const dLost = Math.max(0, lost - prev.lost);
      const dSent = Math.max(0, sent - prev.sent);
      const dRemoteLost = Math.max(0, remoteLost - prev.remoteLost);

      // The percentage is only recomputed once enough packets have gone by,
      // not once per poll. With DTX a silent stream carries a handful of
      // comfort-noise packets a second, and dividing by that few would turn a
      // single lost packet into "50 % loss" and drop the connection bars to
      // one for everyone who is not currently talking. Deltas accumulate into
      // the next window instead of being dropped, so real loss on a quiet
      // link still surfaces — it just takes a couple of seconds. While
      // somebody talks the stream fills a window in well under a second, so
      // the bars stay as responsive as they were.
      const window = this.lossWindows[id] ?? {
        received: 0,
        lost: 0,
        sent: 0,
        remoteLost: 0,
        inbound: 0,
        outbound: 0
      };
      window.received += dReceived;
      window.lost += dLost;
      window.sent += dSent;
      window.remoteLost += dRemoteLost;
      const inboundSample = window.received + window.lost;
      if (inboundSample >= MIN_LOSS_SAMPLE_PACKETS) {
        window.inbound = (window.lost / inboundSample) * 100;
        window.received = 0;
        window.lost = 0;
      }
      if (window.sent >= MIN_LOSS_SAMPLE_PACKETS) {
        window.outbound = Math.min(100, (window.remoteLost / window.sent) * 100);
        window.sent = 0;
        window.remoteLost = 0;
      }
      this.lossWindows[id] = window;
      const packetLoss = Math.max(window.inbound, window.outbound);

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
      const offer = withOpusFeatures(await pc.createOffer());
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
   * Rejects (without changing any state) when microphone access or the audio
   * graph fails, so a denied permission prompt doesn't leave the manager
   * stuck in a half-joined state that blocks all future joins.
   */
  async join(user: string, channelId: number, peersList: RemotePeer[], info?: VoiceChannelInfo) {
    if (this.userName) return;

    // Acquire the microphone and build the graph before touching any state:
    // these are the only steps that can fail.
    const rawStream = await openMicrophone();
    try {
      this.localStream = await this.setupAudioProcessing(rawStream);
    } catch (error) {
      for (const track of rawStream.getTracks()) track.stop();
      throw error;
    }
    this.watchCaptureTrack(rawStream);
    captureStream.set(rawStream);

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
    resetSpeaking();
    chat.on('voice-join', (m) => this.handleJoin(m, peersList));
    chat.on('voice-offer', (m) => this.handleOffer(m, peersList));
    chat.on('voice-answer', (m) => this.handleAnswer(m));
    chat.on('voice-candidate', (m) => this.handleCandidate(m));
    chat.on('voice-leave', (m) => this.handleLeave(m, peersList));

    this.updateTransmissionMode();
    this.syncGlobalPushToTalk();

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
    this.syncGlobalPushToTalk();
    peersList.length = 0;
    this.emit([]);
    resetSpeaking();
  }

  private handleJoin(msg: Message, peersList: RemotePeer[]) {
    if (
      !this.userName ||
      msg.user === this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;
    this.createPeer(msg.user as string, true, peersList);
    this.playPeerSound(this.joinSound);
  }

  private async handleOffer(msg: Message, peersList: RemotePeer[]) {
    if (
      !this.userName ||
      msg.target !== this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;
    const pc = await this.createPeer(msg.user as string, false, peersList);
    // The peer's description is munged too, not just ours: the encoder takes
    // its DTX/FEC settings from the *remote* side of the negotiation, so this
    // is what guarantees our own uplink stops paying for silence even if the
    // offer arrived without the parameters.
    await pc.setRemoteDescription(
      new RTCSessionDescription(withOpusFeatures(msg.sdp as RTCSessionDescriptionInit))
    );
    const answer = withOpusFeatures(await pc.createAnswer());
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
      await pc.setRemoteDescription(
        new RTCSessionDescription(withOpusFeatures(msg.sdp as RTCSessionDescriptionInit))
      );
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
    this.playPeerSound(this.leaveSound);
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

    resetSpeaking();
  }
}
