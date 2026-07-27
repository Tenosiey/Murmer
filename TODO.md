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
- [ ] Role icons — assign an icon (custom server emoji or uploaded image) per
      role in the Server Dashboard, shown next to the user name in the member
      list and messages (roles already carry a color)
- [ ] Text-to-speech
- [ ] User nicknames per server
- [ ] User profile pages (avatars are done — uploaded per server and shown in
      messages, the member list and DMs; a profile view is not)

### 📚 Channel Wiki

The per-channel Markdown wiki is built: pages, `[[wikilinks]]` across channels,
role-gated editing, FTS5 indexing and sanitised rendering. What is left is
surfacing two things the server already stores.

- [ ] Revision history view with diff and restore — the server keeps previous
      versions in `wiki_revisions`, but the client only uses the revision
      counter for its save compare-and-swap
- [ ] Wiki pages in the search UI results — they are already indexed in FTS5
      alongside messages, the overlay just does not show them

### 🎤 Voice Features

- [ ] Automatic input sensitivity — track the noise floor and derive the VAD
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
- [ ] Opus DTX and inband FEC via SDP munging — DTX stops spending bandwidth on
      silence, FEC noticeably improves a lossy link. Small diff, real gain
- [ ] Output limiter / loudness normalisation — a `DynamicsCompressorNode` on
      the remote graph to tame the one person who is always clipping, without
      having to ride their per-user volume by hand
- [ ] Real-time transcription of voice to text
- [ ] Record and play back voice messages
- [ ] RNNoise via AudioWorklet + WASM — genuinely better than the browser's
      built-in `noiseSuppression` constraint, and the most audible quality
      upgrade available here
- [ ] Screen-share annotations
- [ ] Separate volume for the app sounds (join, leave, mute) independent of the
      voice volume slider, which currently drives both
- [ ] Temporary voice channels
- [ ] VAD hold / release-delay slider — the detector already holds the gate
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

- [ ] Admin dashboard — the shell and most tabs are built (overview, emojis,
      moderation, stats, files & uploads, voice, screen share, roles, danger
      zone). Still marked "Coming soon" inside them: slow mode, max message
      length, profanity filter, ban list, default bitrate/quality, purge all
      messages and reset server. There is no online-users or storage-usage view
- [ ] Anonymous chat modes
- [ ] Backup & export of chat history and uploads
- [ ] Decentralized/mesh networking option
- [ ] End-to-end encryption for private channels (DMs are already E2E; channels
      are not)
- [ ] Mini-games embedded in chat
- [ ] Music streaming from local files
- [ ] Pomodoro timer integration for study groups
- [ ] Real-time collaborative code editing
- [ ] Scheduled voice events / calendar integration
- [ ] Translation services for international teams
- [ ] Web client (browser build without Tauri, join via invite link)

---

## 🔧 Tech debt / hardening

- [ ] Add a JS test runner (vitest) to the client — store logic like the
      per-server unread namespacing and the wiki request tracking is complex
      enough to deserve tests
- [ ] Gate the `/upload` endpoint behind authentication (or at least IP rate
      limiting like auth) — currently anyone who can reach the server can
      write 10 MB files to disk; needs a small client change to send
      credentials with the upload
- [ ] Make the rate limiter's clock injectable so the map-sweep behaviour in
      `security.rs` can be covered by a regression test in
      `tests/security_limits.rs` (the 60 s window uses `std::time::Instant`
      directly and cannot be fast-forwarded)

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
