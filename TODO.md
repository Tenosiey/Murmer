# 📝 TODO List

An overview of planned work for the project. Entries are sorted alphabetically
within each section.

Finished work is **removed** from this list rather than ticked off — git
history is the record of what shipped, and a list that only holds open items
stays readable. Use the checkboxes to mark something you have picked up.

---

## 🚀 Features

### 🗨️ Chat Features

- [ ] Bot integration/webhooks (a basic REST bot API exists; webhooks do not)
- [ ] Text-to-speech

### 🎤 Voice Features

- [x] Automatic input sensitivity — track the noise floor and derive the VAD
      threshold from it instead of asking the user to dial in a number, with a
      manual override for the cases it gets wrong
- [ ] Breakout rooms
- [ ] Collaborative whiteboard during voice chats
- [ ] Ducking — drop the soundboard (and other app sounds) while somebody is
      actually talking, so a clip never buries the conversation
- [ ] Gesture recognition through webcam
- [ ] Live polling during meetings
- [ ] Meeting notes that auto-generate from voice
- [ ] Optional spatial/3D audio
- [ ] Output limiter / loudness normalisation — a `DynamicsCompressorNode` on
      the remote graph to tame the one person who is always clipping, without
      having to ride their per-user volume by hand
- [ ] Real-time transcription of voice to text
- [ ] Record and play back voice messages
- [ ] Screen-share annotations
- [x] Separate volume for the app sounds (join, leave, mute) independent of the
      voice volume slider, which currently drives both
- [ ] Temporary voice channels
- [x] VAD hold / release-delay slider — the detector already holds the gate
      open after speech stops, but the timings are the fixed `HOLD_TIME_MS` and
      `RELEASE_DELAY_MS` constants in `voice/vad.ts`; this exposes them the way
      Discord's "PTT release delay" does
- [ ] Virtual backgrounds
- [ ] Voice activity heatmaps
- [ ] Voice-controlled commands
- [ ] Voice effects and filters
- [ ] Voice sentiment analysis
- [ ] Webcam/video in voice channels

### 🛠️ Other Features

- [ ] Anonymous chat modes
- [ ] Backup & export of chat history and uploads
- [ ] Decentralized/mesh networking option
- [ ] Mini-games embedded in chat
- [ ] Music streaming from local files
- [ ] Pomodoro timer integration for study groups
- [ ] Real-time collaborative code editing
- [ ] Scheduled voice events / calendar integration
- [ ] Translation services for international teams

---

## 🔧 Tech debt / hardening

- [ ] Reject wrong-length peer keys in `dm-crypto.ts::dhKeys` —
      `ed2curve.convertPublicKey` does not check its input length, so
      `encryptDm` accepts a truncated key and produces a ciphertext nobody can
      open: the sender believes the DM went out while the peer sees a permanent
      decrypt-failure placeholder. Low severity (a malicious server
      substituting a *valid* key already reads those DMs, and TOFU pinning
      limits both to first contact), but the failure mode should be honest.
      `dm-crypto.test.ts` has a test named for the current behaviour — delete
      it with the fix
- [ ] TURN support — voice does not connect at all behind symmetric NAT or a
      network that blocks UDP, and both managers hardcode one public STUN
      server with no way for an operator to change it. Designed but not
      scheduled: see [`docs/turn-support.md`](docs/turn-support.md) for the
      work breakdown, the ephemeral-credential scheme, the interaction with
      `webrtc/recovery.ts` and the open questions. The first step (making the
      ICE configuration configurable at all) is small and independently useful

---

## 🐛 Bugs

- [ ] Crackling/popping artefacts in transmitted voice audio while speaking —
      the transmission gate now ramps instead of stepping, which removed the
      clicks at the start and end of each burst; needs a re-test to see whether
      anything remains mid-speech
- [ ] Screen share: after the streamer stops sharing, the overlay controls stop
      responding (state is not reset)
- [ ] Soundboard: uploading a new sound fails

---

## 💡 Future Ideas

- [ ] AI-powered chat summarization
- [ ] Federation between Murmer servers (cross-server DMs)
- [ ] Offline LAN party mode without Internet
- [ ] Proximity voice channels for events
