/**
 * WebRTC voice chat manager.
 *
 * Handles peer connection setup using messages sent over the WebSocket chat
 * channel. Consumers subscribe to updates to receive the list of remote peers
 * currently connected.
 *
 * Camera video rides these same connections — see `setCameraTrack` for why it
 * is not a mesh of its own.
 *
 * Above the server's SFU threshold the same call runs over one connection to
 * the server instead (`sfu.ts`). The microphone chain and the camera track
 * do not care which: only what the outgoing tracks are attached to changes,
 * and `mode.ts` decides when the switch happens.
 */
import { chat } from '../stores/chat';
import { iceServers } from '../stores/iceConfig';
import {
  appSoundVolume,
  inputDeviceId,
  outputDeviceId,
  microphoneMuted,
  outputMuted,
  voiceMode,
  vadSensitivity,
  vadAutoSensitivity,
  vadReleaseDelay,
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
import type { ConnectionStats, Message, RemotePeer, VoiceChannelInfo } from '../types';
import { VoiceActivityDetector, type VadConfig } from './vad';
import { PushToTalkManager } from './ptt';
import { getAudioContext, resumeAudioContext } from './audioContext';
import { subscribeTick } from './ticker';
import { micProcessingConstraints, openMicrophone } from './capture';
import { connectMicSource, type MicSource } from './denoise';
import { withOpusFeatures } from './sdp';
import { capBitrate } from '../webrtc/bitrate';
import { remoteFingerprint } from '../webrtc/fingerprint';
import { PeerRecovery } from '../webrtc/recovery';
import { SfuConnection } from './sfu';
import {
  INITIAL_TRANSPORT,
  parseSfuSlots,
  parseVoiceMode,
  stepTransport,
  type TransportAction,
  type TransportEvent
} from './mode';
import { viaServer } from '../stores/voiceTransport';

const DEFAULT_AUDIO_BITRATE = 64_000;

/**
 * Encoder cap for the camera, in bits per second, until the store pushes the
 * chosen quality down. Matches the default preset in `camera.ts`; a peer that
 * connects in the moment between the camera opening and the settings landing
 * would otherwise encode uncapped.
 */
const DEFAULT_CAMERA_BITRATE = 800_000;

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

/**
 * How long a call switching back from the SFU waits for every mesh peer to
 * connect before it drops the SFU anyway. A pair that cannot connect would
 * otherwise keep the whole channel on the server forever; past this it is
 * left to the normal repair path, and reported once `P2P_CONNECT_TIMEOUT_MS`
 * runs out.
 */
const MESH_READY_TIMEOUT_MS = 10_000;
/**
 * How long a new mesh connection gets to reach `connected` before the pair
 * is reported as unreachable (`voice-p2p-failed`) and the server moves the
 * channel onto its SFU. Long enough for a slow ICE gathering, short enough
 * that the people who cannot hear each other are not left wondering.
 */
const P2P_CONNECT_TIMEOUT_MS = 15_000;

/** The packet counters `scoreStats` turns into a loss percentage. */
interface StatsSample {
  rtt: number;
  jitter: number;
  received: number;
  lost: number;
  sent: number;
  remoteLost: number;
}

/** The detector's settings as they currently stand, read from the stores. */
function vadConfig(): VadConfig {
  return {
    sensitivity: get(vadSensitivity),
    automatic: get(vadAutoSensitivity),
    releaseMs: get(vadReleaseDelay)
  };
}

export class VoiceManager {
  private peers: Record<string, RTCPeerConnection> = {};
  /** One stats poll for the whole call, so a tick re-renders the list once. */
  private statsInterval: number | null = null;
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
  private sfuPrevPacketCounts: VoiceManager['prevPacketCounts'] = {};
  private sfuLossWindows: VoiceManager['lossWindows'] = {};
  private localStream: MediaStream | null = null;
  /** The signaling handlers of the current call, so leaving removes exactly
   *  these and not a store's listener for the same frame type. */
  private signaling: [string, (msg: Message) => void][] = [];
  private userName: string | null = null;
  private channelId: number | null = null;
  private listeners: Array<(peers: RemotePeer[]) => void> = [];

  /** Sender of each peer's pre-negotiated camera transceiver. */
  private videoSenders: Record<string, RTCRtpSender> = {};
  /**
   * Incoming camera stream per peer, kept and updated in place rather than
   * rebuilt: handing a video element a new `MediaStream` re-attaches its
   * source and restarts playback from black.
   */
  private remoteVideo: Record<string, MediaStream> = {};
  /** Our camera track while the camera is on, attached to every peer. */
  private cameraTrack: MediaStreamTrack | null = null;
  /** Encoder cap for the camera, in bits per second. */
  private cameraBitrate = DEFAULT_CAMERA_BITRATE;

  /** The peer list of the current session. Empty while not in a channel. */
  private activePeers: RemotePeer[] = [];

  /** Keeps peers whose connection breaks alive instead of dropping them. */
  private recovery = new PeerRecovery({
    restart: (id) => void this.restartPeerIce(id),
    rebuild: (id) => this.rebuildPeer(id),
    setReconnecting: (id, reconnecting) => this.markReconnecting(id, reconnecting)
  });

  /** Which transport the channel wants and which one carries the call. */
  private transport = INITIAL_TRANSPORT;
  /** Receive slots an SFU offer carries, from the last mode frame. */
  private sfuSlots = 0;
  private sfu: SfuConnection | null = null;
  /**
   * The SFU connection's own repair, on the same policy as a mesh peer. A
   * separate controller rather than a reserved peer id, because any id
   * could also be somebody's account name.
   */
  private sfuRecovery = new PeerRecovery({
    restart: () => void this.sfu?.offer(true).catch(() => {}),
    // A fresh offer carries a new certificate, which tells the server to
    // replace its end rather than restart it.
    rebuild: () => this.openSfu(),
    setReconnecting: (_, reconnecting) => this.sfu?.setReconnecting(reconnecting)
  });
  /** Pending `voice-p2p-failed` report per mesh peer not yet connected. */
  private p2pWatch: Record<string, ReturnType<typeof setTimeout>> = {};
  /** Pending give-up on a mesh that is taking over from the SFU. */
  private meshWait: ReturnType<typeof setTimeout> | null = null;

  // The manager is a module singleton that lives as long as the app, so these
  // are never torn down.
  private readonly vad = new VoiceActivityDetector();
  private readonly ptt = new PushToTalkManager(get(pttKey));

  private audioContext: AudioContext | null = null;
  private gainNode: GainNode | null = null;
  /**
   * Input volume, applied ahead of the gate so voice detection and the level
   * meters see the same signal the peers receive.
   */
  private inputGainNode: GainNode | null = null;
  /** Microphone end of the chain, including the RNNoise node when enabled. */
  private micSource: MicSource | null = null;
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

  /** Encoder cap for our microphone in this channel; `null` lifts it. */
  private audioBitrate: number | null = null;

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

    voiceMode.subscribe(() => {
      this.updateTransmissionMode();
      this.syncGlobalPushToTalk();
    });
    vadSensitivity.subscribe(() => this.updateVadConfig());
    vadAutoSensitivity.subscribe(() => this.updateVadConfig());
    vadReleaseDelay.subscribe(() => this.updateVadConfig());
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
    pttKey.subscribe((key) => this.ptt.setKey(key));

    this.vad.subscribe((isActive, level) => {
      voiceActivity.set(isActive);
      this.updateTransmissionState();
    });

    this.ptt.subscribe((isPressed) => {
      isPttActive.set(isPressed);
      this.updateTransmissionState();
    });

    chat.on('voice-channel-update', (msg) => {
      const chId = msg.channelId;
      if (typeof chId !== 'number' || this.channelId !== chId) return;
      if (msg.bitrate === null) {
        this.audioBitrate = null;
      } else if (typeof msg.bitrate === 'number' && Number.isFinite(msg.bitrate)) {
        this.audioBitrate = Math.max(0, Math.round(msg.bitrate));
      }
      this.applyAudioBitrateToPeers();
    });
  }

  private updateTransmissionMode() {
    const source = this.inputGainNode;
    if (!source) return;

    const mode = get(voiceMode);

    if (mode === 'vad') {
      this.vad.start(source, vadConfig());
    } else {
      this.vad.stop();
    }

    this.updateTransmissionState();
  }

  /**
   * Hold the OS-level push-to-talk grab only while it can actually do
   * something: in a channel, in push-to-talk mode.
   */
  private syncGlobalPushToTalk() {
    this.ptt.setGlobalEnabled(this.userName !== null && get(voiceMode) === 'ptt');
  }

  private updateVadConfig() {
    if (get(voiceMode) === 'vad') {
      this.vad.configure(vadConfig());
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

    if (!this.gainNode) return;
    // Ramp instead of stepping: an instant gain change clicks on every open
    // and close of the gate. The ramp is linear rather than exponential
    // because closing the gate has to reach exactly zero — an exponential
    // approach would leave the microphone very quietly open forever.
    rampGain(this.gainNode, shouldTransmit ? 1.0 : 0.0, GATE_RAMP_SECONDS);
  }

  /** Push the configured input volume onto the live graph. */
  private applyInputGain() {
    if (!this.inputGainNode) return;
    rampGain(this.inputGainNode, clampMicGain(get(micGain)), INPUT_GAIN_RAMP_SECONDS);
  }

  /**
   * Cap a camera sender and tell the encoder what to give up first.
   *
   * `maintain-framerate` because a talking head reads fine soft and reads
   * badly stuttering — the opposite trade-off from a screen share, where the
   * text has to stay sharp.
   */
  private configureVideoSender(sender: RTCRtpSender) {
    capBitrate(sender, this.cameraBitrate, 'maintain-framerate');
  }

  /**
   * Attach (or clear) our camera on every peer.
   *
   * `replaceTrack` on the transceiver the connection already negotiated,
   * which is what keeps a camera toggle from being an offer: no
   * renegotiation, no glare, and nothing for a peer mid-repair to answer.
   * Peers that connect later pick the track up as they are built, in
   * `createPeer` or `adoptVideoTransceiver` depending on which end offered.
   */
  setCameraTrack(track: MediaStreamTrack | null, maxBitrate?: number) {
    this.cameraTrack = track;
    if (maxBitrate && maxBitrate > 0) this.cameraBitrate = maxBitrate;
    for (const sender of Object.values(this.videoSenders)) {
      sender.replaceTrack(track).catch((error) => {
        console.error('Failed to update the outgoing camera track:', error);
      });
      if (track) this.configureVideoSender(sender);
    }
    this.sfu?.setCamera(track);
  }

  /**
   * Claim the video m-line of an offer we are answering.
   *
   * The answerer cannot pre-create this transceiver (see `createPeer`), so it
   * arrives from `setRemoteDescription` as `recvonly`. Turning it `sendrecv`
   * *before* the answer is what pre-negotiates our own camera: the direction
   * is settled in this very round of offer/answer, and switching the camera
   * on later stays a `replaceTrack` with nothing to renegotiate.
   */
  private adoptVideoTransceiver(id: string, pc: RTCPeerConnection) {
    // `mid` is what says a transceiver actually belongs to an m-line.
    const video = pc
      .getTransceivers()
      .find((t) => t.mid !== null && t.receiver.track?.kind === 'video');
    if (!video) return;
    video.direction = 'sendrecv';
    this.videoSenders[id] = video.sender;
    if (this.cameraTrack) {
      video.sender.replaceTrack(this.cameraTrack).catch((error) => {
        console.error('Failed to attach the camera to an answered connection:', error);
      });
    }
    this.configureVideoSender(video.sender);
  }

  /**
   * The stream carrying `id`'s camera, holding `track`. One stream per peer,
   * reused for the lifetime of the connection — the transceiver is
   * pre-negotiated, so a peer switching their camera off and on again swaps
   * the track behind this without `ontrack` firing a second time.
   */
  private trackRemoteVideo(id: string, track: MediaStreamTrack): MediaStream {
    let stream = this.remoteVideo[id];
    if (!stream) {
      stream = new MediaStream();
      this.remoteVideo[id] = stream;
    }
    if (!stream.getTrackById(track.id)) {
      for (const existing of stream.getVideoTracks()) stream.removeTrack(existing);
      stream.addTrack(track);
    }
    return stream;
  }

  private applyAudioBitrateToPeers() {
    for (const pc of Object.values(this.peers)) {
      for (const sender of pc.getSenders()) {
        if (sender.track?.kind === 'audio') capBitrate(sender, this.audioBitrate);
      }
    }
    this.sfu?.setAudioBitrate(this.audioBitrate);
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
      this.startLocalLevelMeter();
      return destination.stream;
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

  /**
   * Hand subscribers the peers of whichever transport is live. The other
   * one's peers are never listed: while a switch is under way both carry
   * the same voices, and listing both would play everybody twice.
   */
  private publish() {
    const live = this.transport.live === 'sfu' && this.sfu ? this.sfu.peers() : this.activePeers;
    this.emit([...live]);
  }

  /** Feed an event to the transport rules and carry out what they say. */
  private advanceTransport(event: TransportEvent) {
    const wasLive = this.transport.live;
    const { state, actions } = stepTransport(this.transport, event);
    this.transport = state;
    for (const action of actions) this.runTransportAction(action);
    if (state.live !== wasLive) {
      viaServer.set(state.live === 'sfu' ? { rtt: 0 } : null);
      this.publish();
    }
  }

  private runTransportAction(action: TransportAction) {
    switch (action) {
      case 'open-sfu':
        this.openSfu();
        break;
      case 'close-sfu':
        this.closeSfu();
        break;
      case 'close-mesh':
        this.clearMeshWait();
        for (const id of Object.keys(this.peers)) this.cleanupPeer(id);
        break;
      case 'build-mesh': {
        // The SFU's member list is exactly who we need a connection to.
        // The usual tiebreak picks who offers; the other end answers.
        const members = this.sfu?.peers().map((p) => p.id) ?? [];
        for (const id of members) {
          if (this.userName && this.userName > id) void this.createPeer(id, true);
        }
        this.clearMeshWait();
        this.meshWait = setTimeout(() => {
          this.meshWait = null;
          this.advanceTransport({ type: 'mesh-ready' });
        }, MESH_READY_TIMEOUT_MS);
        this.checkMeshReady();
        break;
      }
    }
  }

  /** Open (or replace) the SFU connection with a fresh offer. */
  private openSfu() {
    if (!this.userName || this.channelId === null) return;
    this.closeSfu();
    const sfu = new SfuConnection({
      channelId: this.channelId,
      slots: this.sfuSlots,
      audio: this.localStream?.getAudioTracks()[0] ?? null,
      camera: this.cameraTrack,
      audioBitrate: this.audioBitrate,
      send: (frame) => chat.sendRaw(frame),
      onPeers: () => {
        if (this.transport.live === 'sfu') this.publish();
      },
      onState: (state) => {
        if (this.sfu !== sfu) return;
        this.sfuRecovery.observe('sfu', state);
        if (state === 'connected') this.advanceTransport({ type: 'sfu-connected' });
      },
      configureVideo: (sender) => this.configureVideoSender(sender)
    });
    this.sfu = sfu;
    sfu.offer().catch((error) => {
      console.error('Failed to offer the SFU connection:', error);
    });
  }

  private closeSfu() {
    this.sfuRecovery.clear();
    this.sfu?.close();
    this.sfu = null;
    this.sfuPrevPacketCounts = {};
    this.sfuLossWindows = {};
  }

  private clearMeshWait() {
    if (this.meshWait !== null) {
      clearTimeout(this.meshWait);
      this.meshWait = null;
    }
  }

  /** Hand the call back to the mesh once every member is connected on it. */
  private checkMeshReady() {
    if (this.transport.target !== 'mesh' || this.transport.live !== 'sfu' || !this.sfu) return;
    const ready = this.sfu
      .peers()
      .every((p) => this.peers[p.id]?.connectionState === 'connected');
    if (ready) {
      this.clearMeshWait();
      this.advanceTransport({ type: 'mesh-ready' });
    }
  }

  private handleVoiceMode(msg: Message) {
    const frame = parseVoiceMode(msg);
    if (!frame || frame.channelId !== this.channelId) return;
    this.sfuSlots = frame.slots;
    this.advanceTransport({ type: 'mode', mode: frame.mode });
  }

  private handleSfuAnswer(msg: Message) {
    if (msg.channelId !== this.channelId || typeof msg.sdp !== 'string') return;
    this.sfu?.answer(msg.sdp).catch((error) => {
      console.error('Failed to apply the SFU answer:', error);
    });
  }

  private handleSfuSlots(msg: Message) {
    const frame = parseSfuSlots(msg);
    if (!frame || frame.channelId !== this.channelId) return;
    this.sfu?.setSlots(frame.slots);
  }

  private cleanupPeer(id: string) {
    this.recovery.forget(id);
    this.unwatchP2p(id);
    const pc = this.peers[id];
    if (pc) {
      pc.close();
      delete this.peers[id];
      delete this.prevPacketCounts[id];
      delete this.lossWindows[id];
    }
    delete this.videoSenders[id];
    delete this.remoteVideo[id];
    // Remove from `activePeers` itself, the array every emit spreads: a peer
    // only filtered out of the copy handed to subscribers comes back the
    // moment anything else emits — a closed connection reappearing in the
    // member list with a dead stream.
    this.activePeers = this.activePeers.filter((r) => r.id !== id);
    this.publish();
  }

  /**
   * Offer a broken peer a fresh ICE transport over whatever path is reachable
   * now. Both ends see the same breakage, so exactly one of them may offer or
   * the two renegotiations collide; comparing the two names picks the same
   * side on both machines without needing a round trip to agree. The other end
   * answers the offer that arrives through the normal `voice-offer` path.
   */
  private async restartPeerIce(id: string) {
    const pc = this.peers[id];
    if (!pc || !this.userName || pc.signalingState === 'closed') return;
    // A mesh on its way out for the SFU is not worth repairing.
    if (this.transport.target !== 'mesh') return;
    // Names are unique per server, so this is never a tie.
    if (this.userName < id) return;
    try {
      pc.restartIce();
      await this.sendOffer(id, pc);
    } catch (error) {
      // A restart that throws is just an attempt that did not land; the
      // controller retries, and the deadline still ends in a rebuild.
      if (import.meta.env.DEV) console.error('ICE restart failed', error);
    }
  }

  /**
   * Throw a connection away and start a new one. This is where a peer ends up
   * after the repair deadline: their client may have been offline for the
   * whole window and come back on a different network, which no amount of ICE
   * restarting on a dead transport recovers from. Nothing is scheduled after
   * this — if the peer really left, the server's `voice-leave` removes them,
   * and until then the new connection retries on its own timers.
   */
  private rebuildPeer(id: string) {
    if (!this.userName || !this.peers[id]) return;
    this.cleanupPeer(id);
    if (this.transport.target !== 'mesh') return;
    // Same tiebreak as the ICE restart: one side offers, the other picks the
    // new session up from that offer.
    if (this.userName > id) {
      void this.createPeer(id, true);
    }
  }

  /**
   * Report a mesh pair that does not connect in time, so the server carries
   * the channel instead: the relay of last resort since TURN was dropped.
   * Every new connection is watched, rebuilds included, so a pair whose
   * repair keeps failing is caught by the same rule as one that never
   * connected. The server ignores the report when it runs no SFU.
   */
  private watchP2p(id: string) {
    this.unwatchP2p(id);
    this.p2pWatch[id] = setTimeout(() => {
      delete this.p2pWatch[id];
      if (this.transport.target !== 'mesh' || this.peers[id]?.connectionState === 'connected')
        return;
      chat.sendRaw({ type: 'voice-p2p-failed', channelId: this.channelId, target: id });
    }, P2P_CONNECT_TIMEOUT_MS);
  }

  private unwatchP2p(id: string) {
    clearTimeout(this.p2pWatch[id]);
    delete this.p2pWatch[id];
  }

  /** Flag a peer as under repair so the UI can say so instead of going quiet. */
  private markReconnecting(id: string, reconnecting: boolean) {
    const peer = this.activePeers.find((p) => p.id === id);
    if (!peer || peer.reconnecting === reconnecting) return;
    peer.reconnecting = reconnecting;
    this.publish();
  }

  /** Refresh every peer's connection stats, then publish the list once. */
  private async pollStats() {
    await Promise.all([
      ...Object.keys(this.peers).map((id) => this.updateStats(id)),
      this.updateSfuStats()
    ]);
    this.publish();
  }

  /**
   * Stats for the SFU connection: one round-trip time, to the server, and
   * loss and jitter per member from the receive slot carrying their audio,
   * so the bars beside each name still say something about that member.
   */
  private async updateSfuStats() {
    const sfu = this.sfu;
    if (!sfu) return;
    const peers = sfu.peers();
    if (sfu.pc.connectionState !== 'connected') {
      for (const p of peers) p.stats = { rtt: 0, jitter: 0, packetLoss: 0, strength: 0 };
      return;
    }
    try {
      const reports = await sfu.pc.getStats();
      let rtt = 0;
      let sent = 0;
      let remoteLost = 0;
      const inbound: Record<string, { received: number; lost: number; jitter: number }> = {};
      reports.forEach((report) => {
        if (
          report.type === 'candidate-pair' &&
          (report as any).state === 'succeeded' &&
          (report as any).currentRoundTripTime != null
        ) {
          rtt = (report as any).currentRoundTripTime * 1000;
        }
        if (report.type === 'remote-inbound-rtp' && (report as any).kind === 'audio') {
          remoteLost += (report as any).packetsLost ?? 0;
        }
        if (report.type === 'outbound-rtp' && (report as any).kind === 'audio') {
          sent += (report as any).packetsSent ?? 0;
        }
        if (report.type === 'inbound-rtp' && (report as any).kind === 'audio') {
          const user = sfu.audioUser((report as any).mid);
          if (user) {
            inbound[user] = {
              received: (report as any).packetsReceived ?? 0,
              lost: (report as any).packetsLost ?? 0,
              jitter: ((report as any).jitter ?? 0) * 1000
            };
          }
        }
      });
      for (const p of peers) {
        const own = inbound[p.id] ?? { received: 0, lost: 0, jitter: 0 };
        p.stats = this.scoreStats(p.id, { rtt, sent, remoteLost, ...own }, true);
      }
      if (this.sfu === sfu && this.transport.live === 'sfu') viaServer.set({ rtt });
    } catch {
      // ignore stats errors
    }
  }

  private async updateStats(id: string) {
    const pc = this.peers[id];
    if (!pc) return;
    // A connection that is not up reports no round-trip time, and `rtt === 0`
    // is read as "excellent" further down — so a peer being repaired, or one
    // that never connected at all, would sit there showing five full bars
    // while carrying no audio. Report it as what it is instead.
    if (pc.connectionState !== 'connected') {
      for (const p of this.activePeers) {
        if (p.id === id) p.stats = { rtt: 0, jitter: 0, packetLoss: 0, strength: 0 };
      }
      return;
    }
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

      const stats = this.scoreStats(id, { rtt, jitter, received, lost, sent, remoteLost });
      for (const p of this.activePeers) {
        if (p.id === id) p.stats = stats;
      }
    } catch {
      // ignore stats errors
    }
  }

  /**
   * Turn one poll's cumulative counters for `id` into the stats the UI
   * shows: windowed loss in both directions, and bars from rtt and loss.
   */
  private scoreStats(id: string, sample: StatsSample, viaSfu = false): ConnectionStats {
    const { rtt, jitter, received, lost, sent, remoteLost } = sample;
    // The SFU's counters are its own: during a switch the same member has
    // a mesh connection too, and mixing the two would make nonsense deltas.
    const prevCounts = viaSfu ? this.sfuPrevPacketCounts : this.prevPacketCounts;
    const lossWindows = viaSfu ? this.sfuLossWindows : this.lossWindows;
    // Loss in both directions: packets we didn't receive plus packets the
    // peer reports missing from us. Deltas are clamped because the
    // cumulative counters may decrease (e.g. after duplicate packets or an
    // SSRC restart).
    const prev = prevCounts[id] ?? { received: 0, lost: 0, sent: 0, remoteLost: 0 };
    prevCounts[id] = { received, lost, sent, remoteLost };
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
    const window = lossWindows[id] ?? {
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
    lossWindows[id] = window;
    const packetLoss = Math.max(window.inbound, window.outbound);

    let strength =
      rtt === 0 ? 5 : rtt < 50 ? 5 : rtt < 100 ? 4 : rtt < 200 ? 3 : rtt < 400 ? 2 : 1;
    // Heavy packet loss ruins a call even on a fast link, so cap the bars.
    if (packetLoss >= 10) strength = Math.min(strength, 1);
    else if (packetLoss >= 5) strength = Math.min(strength, 2);
    else if (packetLoss >= 2) strength = Math.min(strength, 3);

    return { rtt, jitter, packetLoss, strength };
  }

  private async createPeer(id: string, initiator: boolean): Promise<RTCPeerConnection> {
    if (this.peers[id]) return this.peers[id];
    const pc = new RTCPeerConnection({ iceServers: get(iceServers) });
    this.peers[id] = pc;
    this.watchP2p(id);
    if (this.localStream) {
      for (const track of this.localStream.getTracks()) {
        const sender = pc.addTrack(track, this.localStream);
        if (track.kind === 'audio') capBitrate(sender, this.audioBitrate);
      }
    }
    // Every voice connection carries a camera transceiver whether or not a
    // camera is on. A transceiver that already exists takes a track through
    // `replaceTrack`, which needs no renegotiation — so switching a camera on
    // is not an offer, and two peers doing it at the same instant cannot
    // collide. Renegotiating per toggle would be exactly the glare neither
    // this manager nor `handleOffer` has a rollback for.
    //
    // Only the side that offers may create it. A transceiver from
    // `addTransceiver` is **never** matched to an incoming offer's m-line —
    // only `addTrack` ones are — so creating it here on the answering side
    // left the camera on a transceiver with no `mid`, silently sending to
    // nobody while the offer's video line got a fresh recvonly one.
    // `adoptVideoTransceiver` takes that one over instead.
    if (initiator) {
      const video = pc.addTransceiver(this.cameraTrack ?? 'video', { direction: 'sendrecv' });
      this.videoSenders[id] = video.sender;
      this.configureVideoSender(video.sender);
    }
    pc.ontrack = (ev) => {
      const existing = this.activePeers.find((r) => r.id === id);
      // The camera transceiver is created rather than added with a track, so
      // it has no stream association and `ev.streams` is empty for video.
      const peer = existing ?? { id, stream: new MediaStream() };
      if (ev.track.kind === 'video') {
        peer.video = this.trackRemoteVideo(id, ev.track);
      } else if (ev.streams[0]) {
        peer.stream = ev.streams[0];
      }
      if (!existing) this.activePeers.push(peer);
      this.publish();
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
      // `disconnected` and `failed` are handed to the repair controller rather
      // than acted on here: neither is a reason to lose the peer, and the
      // rules for when they become one are in `recovery.ts`.
      this.recovery.observe(id, pc.connectionState);
      // `closed` is only ever reached because we closed it ourselves, but the
      // peer still has to leave the list when that happened elsewhere.
      if (pc.connectionState === 'closed') this.cleanupPeer(id);
      if (pc.connectionState === 'connected') {
        this.unwatchP2p(id);
        this.checkMeshReady();
      }
    };
    if (initiator) await this.sendOffer(id, pc);
    return pc;
  }

  /** Create an offer on `pc` and send it to `id`. */
  private async sendOffer(id: string, pc: RTCPeerConnection) {
    if (!this.userName) return;
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

  /** Whether a signaling frame is addressed to us, in our channel. */
  private addressedToMe(msg: Message): boolean {
    return !!this.userName && msg.target === this.userName && msg.channelId === this.channelId;
  }

  /**
   * Join a voice channel and start streaming the local microphone.
   *
   * Rejects (without changing any state) when microphone access or the audio
   * graph fails, so a denied permission prompt doesn't leave the manager
   * stuck in a half-joined state that blocks all future joins.
   */
  async join(user: string, channelId: number, info?: VoiceChannelInfo) {
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
    this.activePeers = [];
    this.transport = INITIAL_TRANSPORT;
    this.audioBitrate = info ? info.bitrate : DEFAULT_AUDIO_BITRATE;
    this.statsInterval = window.setInterval(() => void this.pollStats(), 1000);
    resetSpeaking();
    this.signaling = [
      ['voice-join', (m) => this.handleJoin(m)],
      ['voice-offer', (m) => this.handleOffer(m)],
      ['voice-answer', (m) => this.handleAnswer(m)],
      ['voice-candidate', (m) => this.handleCandidate(m)],
      ['voice-leave', (m) => this.handleLeave(m)],
      ['voice-permissions', (m) => this.handleVoiceMode(m)],
      ['voice-mode', (m) => this.handleVoiceMode(m)],
      ['sfu-answer', (m) => this.handleSfuAnswer(m)],
      ['sfu-slots', (m) => this.handleSfuSlots(m)]
    ];
    for (const [type, handler] of this.signaling) chat.on(type, handler);

    this.updateTransmissionMode();
    this.syncGlobalPushToTalk();

    chat.sendRaw({ type: 'voice-join', user, channelId });
    this.broadcastMuteState();
  }

  /**
   * Leave the current voice channel and clean up all peer connections.
   */
  leave(channelId: number) {
    if (!this.userName) return;
    chat.sendRaw({ type: 'voice-leave', user: this.userName, channelId });
    // Before the teardown, so no timer can fire a restart or a rebuild against
    // a session that is on its way out.
    this.recovery.clear();
    if (this.statsInterval !== null) {
      clearInterval(this.statsInterval);
      this.statsInterval = null;
    }
    for (const id of Object.keys(this.peers)) {
      this.cleanupPeer(id);
    }
    this.clearMeshWait();
    this.closeSfu();
    this.transport = INITIAL_TRANSPORT;
    viaServer.set(null);
    this.cleanupAudioProcessing();
    this.localStream = null;
    // The capture itself belongs to the webcam store, which stops it on the
    // same path; only the references are dropped here.
    this.cameraTrack = null;
    this.videoSenders = {};
    this.remoteVideo = {};

    this.vad.stop();

    voiceActivity.set(false);
    isPttActive.set(false);

    for (const [type, handler] of this.signaling) chat.off(type, handler);
    this.signaling = [];
    this.userName = null;
    this.channelId = null;
    this.audioBitrate = null;
    this.activePeers = [];
    this.syncGlobalPushToTalk();
    this.emit([]);
    resetSpeaking();
  }

  private handleJoin(msg: Message) {
    if (
      !this.userName ||
      msg.user === this.userName ||
      msg.channelId !== this.channelId
    )
      return;
    // On the SFU a joiner reaches us through the server's slots instead.
    if (this.transport.target === 'mesh') this.createPeer(msg.user as string, true);
    this.playPeerSound(this.joinSound);
  }

  private async handleOffer(msg: Message) {
    if (!this.addressedToMe(msg)) return;
    // A mesh offer that crossed the switch to the SFU: there is nothing
    // left to answer it with that would not be torn down again.
    if (this.transport.target !== 'mesh') return;
    const remote = msg.user as string;
    const offerSdp = (msg.sdp as RTCSessionDescriptionInit | undefined)?.sdp;
    // Two very different offers arrive on an existing connection. An ICE
    // restart is a renegotiation of *this* connection and is answered on it;
    // an offer from a peer that gave up and built a new `RTCPeerConnection`
    // (see `rebuildPeer`) carries a different certificate and can only be
    // answered on a new one of ours. A connection already past saving is
    // replaced either way.
    const existing = this.peers[remote];
    if (existing) {
      const rebuilt =
        existing.currentRemoteDescription != null &&
        remoteFingerprint(offerSdp) !== remoteFingerprint(existing.currentRemoteDescription.sdp);
      if (rebuilt || existing.connectionState === 'failed' || existing.signalingState === 'closed') {
        this.cleanupPeer(remote);
      }
    }
    const pc = await this.createPeer(remote, false);
    // The peer's description is munged too, not just ours: the encoder takes
    // its DTX/FEC settings from the *remote* side of the negotiation, so this
    // is what guarantees our own uplink stops paying for silence even if the
    // offer arrived without the parameters.
    await pc.setRemoteDescription(
      new RTCSessionDescription(withOpusFeatures(msg.sdp as RTCSessionDescriptionInit))
    );
    // Has to happen before the answer: the answer is what tells the peer we
    // will be sending video on that m-line as well as receiving it.
    this.adoptVideoTransceiver(remote, pc);
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
    if (!this.addressedToMe(msg)) return;
    const pc = this.peers[msg.user as string];
    // Accept an answer whenever one is outstanding, not just the first one:
    // an ICE restart is a second round of offer/answer on a connection that
    // already has a remote description, and gating on that description being
    // absent is what would silently drop every repair.
    if (pc && pc.signalingState === 'have-local-offer') {
      await pc.setRemoteDescription(
        new RTCSessionDescription(withOpusFeatures(msg.sdp as RTCSessionDescriptionInit))
      );
    }
  }

  private async handleCandidate(msg: Message) {
    if (!this.addressedToMe(msg)) return;
    const pc = this.peers[msg.user as string];
    if (pc) {
      try {
        await pc.addIceCandidate(msg.candidate as any);
      } catch {}
    }
  }

  private handleLeave(msg: Message) {
    if (!this.userName || msg.channelId !== this.channelId) return;
    this.cleanupPeer(msg.user as string);
    this.playPeerSound(this.leaveSound);
  }
}

/**
 * Ramp a gain node to `target` instead of stepping it, which clicks.
 * Falls back to a step on an engine that refuses the automation.
 */
function rampGain(node: GainNode, target: number, seconds: number) {
  const gain = node.gain;
  const now = node.context.currentTime;
  try {
    gain.cancelScheduledValues(now);
    gain.setValueAtTime(gain.value, now);
    gain.linearRampToValueAtTime(target, now + seconds);
  } catch {
    gain.value = target;
  }
}
