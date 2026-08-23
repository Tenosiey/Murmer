# Voice

The audio pipeline in `murmer_client/src/lib/voice/`: how the microphone
signal is built, how transmission is gated, how the codec is configured, and
how a broken connection is repaired. Screen sharing shares the repair layer
and lives in [`screen-sharing.md`](screen-sharing.md).

Audio is peer-to-peer. Everyone in a voice channel holds a connection to
everyone else and the server relays only signaling, which is why per-peer
cost shows up so often below as the reason something is the way it is.

## One AudioContext

Every graph in the app shares one `AudioContext` (`voice/audioContext.ts`),
because browsers cap how many may exist at once.

It is asked for **48 kHz** specifically: that is the only rate RNNoise is
trained for.

The microphone chain still ends at its own `MediaStreamAudioDestinationNode`,
so playback rendered to the context destination cannot leak into the outgoing
track.

## The outgoing chain

```
capture → noise suppression → input gain → transmission gate → outgoing track
                                    │
                                    └─► voice detection + settings level meter
```

The tap point matters. Voice detection and the settings level meter read the
signal **after** the input gain (the "Input volume" slider), so the level
compared against the VAD threshold is the one the peers actually receive.
Turning a quiet microphone up must not make voice detection *harder* to
trigger.

Three consumers open the microphone — the voice manager, the settings level
meter, and the record-and-play-back mic test — and all three go through
`voice/capture.ts` and build their microphone end through
`denoise.ts::connectMicSource`. That is what makes it impossible for a meter
to show a level measured on a different signal than the one being gated.

### Noise suppression

One of three modes (`stores/settings.ts`'s `noiseSuppressionMode`), **never
two at once**:

- `browser` — ask the platform for its `noiseSuppression` capture constraint;
- `rnnoise` (the default) — run the RNNoise worklet from `voice/denoise.ts`;
- off.

Stacking them would hand the better filter a signal the other one already
mangled.

RNNoise sits **ahead of** the input gain so it sees speech at capture level,
which also collapses the noise floor the VAD tracks.

Three things RNNoise needs, all of which fail quietly if forgotten:

1. The WASM binary is fetched on the main thread and passed in as an
   `ArrayBuffer` — an `AudioWorkletGlobalScope` has no `fetch`.
2. The CSP in `tauri.conf.json` must keep `'wasm-unsafe-eval'` in
   `script-src`. The dev server sends no CSP, so a mistake here only surfaces
   in a packaged build.
3. The shared `AudioContext` must run at 48 kHz.

Every failure path drops the node and keeps the microphone working.

## Voice activity detection

The VAD threshold is **derived from a tracked noise floor** by default
(`NoiseFloorTracker` in `voice/vad.ts`). `vadSensitivity` is only the manual
override used when `vadAutoSensitivity` is off.

The tracker drops fast and rises very slowly, and can never rise above the
quietest level of the last 20 s. Speech returns to the room between
syllables, so that cap is what stops a long sentence from ratcheting the
threshold up underneath the speaker.

**Tune it towards never gating a talking user.** Transmitting a few more
seconds of fan noise is the cheaper mistake.

Each measurement chain runs its own tracker — the detector's and the settings
meter's — on the same signal.

### The release gate

How long the gate stays open after the level drops back below the threshold
is the user's setting (`vadReleaseDelay`, Settings → Voice), applied by
`ReleaseGate` in `voice/vad.ts`. It replaced two fixed constants that were
only ever added together.

It is read **on every tick** rather than latched when the gate opens, so
dragging the slider changes the release already running.

Its bounds and clamp sit in `stores/settings.ts` next to `clampMicGain`,
because `voice/vad.ts` is downstream of the settings store and may not import
back into it. `VAD_RELEASE_MAX_MS` stays under the tracker's `QUIET_DWELL_MS`
so no setting can hold the gate open past the point where the floor may rise
again.

### Why worklet ticks, not rAF

Level meters are driven by an audio-worklet tick (`voice/ticker.ts`) rather
than `requestAnimationFrame`. rAF stops while the window is minimised, which
used to freeze voice-activity detection with the microphone stuck open.

## Codec configuration

Opus is negotiated with `usedtx=1;useinbandfec=1`, written into every
offer and answer by `voice/sdp.ts`.

Those fmtp parameters are **receiver-to-sender** preferences (RFC 7587), so
the description we *send* configures the peer's encoder and the one we
*receive* configures ours — which is why both the local and the remote
description are munged.

The rewrite is a pure, idempotent function that only appends to Opus fmtp
lines and hands back anything it cannot parse. A call must still connect when
munging fails.

DTX pairs with the transmission gate, which feeds the encoder digital zero
while muted or not transmitting: a silent uplink drops from ~50 packets/s to
a handful (measured 50 → 6.7).

That is also why `updateStats` only recomputes packet loss once
`MIN_LOSS_SAMPLE_PACKETS` have gone by instead of once per poll. Dividing by
the two or three packets a DTX'd stream carries per second turned a single
loss into "50 % loss" and emptied the connection bars of everyone who was not
talking.

`updateStats` also reports **zero bars for anything not `connected`**: an rtt
of 0 otherwise reads as "excellent", which showed five full bars for a peer
carrying no audio at all.

## Connection repair

A peer connection that breaks mid-call is **repaired, never dropped**. The
policy lives in `src/lib/webrtc/recovery.ts` and is shared with screen
sharing.

`disconnected` is a Wi-Fi roam or a lid closed for a second and usually heals
itself, so it gets a grace period. Only if it does not resolve — or on
`failed`, which never resolves — is ICE restarted, retried until a deadline,
and finally the connection thrown away and rebuilt.

Closing on `disconnected`, which this replaced, turned every brief hiccup
into a peer that was still listed and carried no audio, or a screen-share
window that shut for good.

The controller is deliberately free of `window`, WebRTC and store imports,
precisely so these timings — the part that fails invisibly — can be
unit-tested against fake timers.

`webrtc/fingerprint.ts` tells the two kinds of re-offer apart: an ICE restart
keeps the DTLS certificate and is answered on the existing connection, while
a peer that rebuilt presents a new one and can only be answered on a new
connection.

**Both managers must clear their recovery entries on every teardown path**,
or a timer fires against a session that is already gone.

Two voice-side rules hold the rest together:

- `handleAnswer` gates on `signalingState === 'have-local-offer'` rather than
  on there being no remote description yet. A repair is a *second* round of
  offer/answer, so the old check silently dropped every one of them.
- Which of the two ends re-offers is decided by comparing the account names,
  so both machines pick the same side without a round trip to agree.

## Soundboard playback

Soundboard sounds are never mixed into a microphone stream. The server
authorizes `play-sound` and fans out a `soundboard-play` frame; each client
fetches and plays the file itself. See [`features.md`](features.md).

## Relay support

Murmer has no TURN/relay support today, so two peers behind symmetric NATs
cannot connect. The design note is [`../plans/turn-support.md`](../plans/turn-support.md).
