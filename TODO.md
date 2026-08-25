# 📝 TODO List

An overview of planned work for the project.

Sections are ordered by what usually gets picked up first — bugs, then
hardening, then performance, then new work. Entries are sorted alphabetically
within each section.

Finished work is **removed** from this list rather than ticked off — git
history is the record of what shipped, and a list that only holds open items
stays readable. Use the checkboxes to mark something you have picked up.

Where an entry names a file or a symbol, that is the place to start reading,
not a description of the fix.

---

## 🐛 Bugs

- [ ] Crackling/popping artefacts in transmitted voice audio while speaking —
      the transmission gate now ramps instead of stepping, which removed the
      clicks at the start and end of each burst; needs a re-test to see
      whether anything remains mid-speech
- [ ] Screen share: after the streamer stops sharing, the overlay controls
      stop responding — the viewer-side state is never reset
- [ ] Soundboard: uploading a new sound fails. Not reproduced from reading the
      code; two leads to rule out first. `validate_sound_name` bounds the name
      in **bytes** while `SoundboardPanel.svelte` bounds it in UTF-16 units,
      so a long name carrying non-ASCII characters passes the client and is
      rejected by the server. Separately, an extension on the `/upload` audio
      list but not on `UPLOAD_SOUND_EXTENSIONS` — `.flac` is the only one —
      uploads fine and is then refused by `add-sound`, leaving the file
      orphaned on disk; the picker's `accept` hides that, "All files" does not

---

## 🔧 Tech debt / hardening

- [ ] Content-Security-Policy for the web client. The desktop shell ships a
      full policy in `tauri.conf.json`; the same bundle served over HTTP gets
      `nosniff`, `referrer-policy` and `x-frame-options` and nothing else. Two
      shipped targets, the same `{@html}` markdown boundary, two different
      security postures — and the browser one is the weaker
- [ ] Lagged broadcast receivers are dropped silently. Both
      `RecvError::Lagged` arms in `ws/handlers/mod.rs` are empty, so a client
      that falls behind the 100-frame channel loses frames with no log, no
      warning and no resync — the message simply never appears for that one
      person. Log the skipped count at minimum; better, tell the client to
      re-request the affected state
- [ ] Mirror-test the soundboard constants. `SOUND_EXTENSIONS`,
      `MAX_SOUND_FILE_BYTES`, `MAX_SOUNDBOARD_SOUNDS`, the name-length bounds
      and the cooldown are all defined on both sides, and
      `test/server-mirror.test.ts` covers permissions, the upload safe-list
      and the chat policy but not these. Drift shows up as an upload that
      fails after the file has already been stored
- [ ] Reclaim orphaned uploads. Deleting an emoji, avatar, server icon or
      sound removes its file; deleting a *message* does not, and neither does
      the Danger Zone purge or reset. The dashboard's storage breakdown can
      only ever grow. Needs a sweep reconciling `uploads/` against the rows
      that reference it
- [ ] Retention policy for message history. The database grows without bound
      and an operator has only the all-or-nothing purge. A server-wide or
      per-channel "delete messages older than N days", cascading through
      reactions, pins and the FTS index, is the counterpart to the upload
      sweep above
- [ ] Serve `/files` as inert content. Uploads come back from the app's own
      origin with no `Content-Disposition` and no sandbox policy, which leaves
      the extension safe-list as the only thing between an upload and script
      execution in that origin. `Content-Disposition: attachment` plus a
      `sandbox` CSP on the route makes the safe-list defence in depth rather
      than the whole defence
- [ ] Split the three files that have outgrown being read end to end:
      `routes/chat/+page.svelte` (2.4k lines), `ServerDashboardModal.svelte`
      (2.3k) and `SettingsModal.svelte` (1.6k). "Keep it simple" cuts both
      ways — past a point the flat file is the complicated option
- [ ] Test the per-recipient frame filter over a real WebSocket. Nothing in
      `murmer_server/tests/` opens `/ws`, so the auth handshake and the
      filtering in the `global_rx` arm — the code deciding whether a private
      channel's messages reach a given connection — are only ever exercised by
      hand. `direct_routing_test.rs` calls the helpers directly and stops
      short of the dispatch loop
- [ ] TURN support — voice does not connect at all behind symmetric NAT or a
      network that blocks UDP, and both managers hardcode one public STUN
      server with no way for an operator to change it. Designed but not
      scheduled: see [`plans/turn-support.md`](plans/turn-support.md) for the
      work breakdown, the ephemeral-credential scheme, the interaction with
      `webrtc/recovery.ts` and the open questions. The first step (making the
      ICE configuration configurable at all) is small and independently useful

---

## ⚡ Performance

- [ ] One array holds every message from every channel. `$chat` is flat and
      unbounded: each update re-filters it for the open channel and rebuilds
      every block, and every message ever scrolled into view stays in the DOM.
      A long session with deep scrollback pays for all of it on every incoming
      message. Key it by channel, cap it, or window the list
- [ ] Parse each broadcast frame once, not once per connection. Each
      connection task runs its own substring scan and `serde_json::from_str`
      over the same global frame, so one DM or channel-scoped frame is parsed
      as many times as there are clients. Deciding the routing at the send
      site and shipping it alongside the frame makes fan-out constant in parse
      cost

---

## 🚀 Features

### 🗨️ Chat Features

- [ ] Outbound webhooks. The bot REST API covers "something else drives
      Murmer"; there is no way round for Murmer to notify something else when
      a message arrives
- [ ] Saved messages — a personal bookmark list, separate from the
      server-wide pins
- [ ] Text-to-speech

### 🎤 Voice Features

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
- [ ] Raise hand and a speaking queue, for the calls with more listeners than
      talkers
- [ ] Real-time transcription of voice to text
- [ ] Record and play back voice messages
- [ ] Screen-share annotations
- [ ] Temporary voice channels
- [ ] Text chat scoped to a voice channel — somewhere to drop a link mid-call
      that does not interrupt the channel everyone else is reading
- [ ] Virtual backgrounds
- [ ] Voice activity heatmaps
- [ ] Voice-controlled commands
- [ ] Voice effects and filters
- [ ] Voice sentiment analysis

### 🛠️ Other Features

- [ ] Accessibility pass — keyboard navigation and screen-reader labels. The
      main chat page carries five `aria-` attributes across 2.4k lines, so
      most of the app is currently hard to reach without a mouse
- [ ] Anonymous chat modes
- [ ] Backup & export of chat history and uploads
- [ ] Decentralized/mesh networking option
- [ ] Mini-games embedded in chat
- [ ] Music streaming from local files
- [ ] Narrow-window and touch layout. The web client is a shipped target, and
      six `max-width` media queries in the whole client is what it has to meet
      a phone with
- [ ] Pomodoro timer integration for study groups
- [ ] Real-time collaborative code editing
- [ ] Scheduled voice events / calendar integration
- [ ] Shareable themes — the accent wheel re-tints, but a theme cannot be
      saved, exported or handed to somebody else
- [ ] Translatable UI. Every string is hardcoded English, so this is a
      structural change (extraction plus a lookup) rather than a translation
      job, and it only gets more expensive with every screen added
- [ ] Translation services for international teams

---

## 💡 Future Ideas

- [ ] AI-powered chat summarization
- [ ] An SFU for large voice channels — the real answer to the mesh's square
      growth, and a much bigger commitment than TURN: it puts media through
      the server, which today never sees any
- [ ] Federation between Murmer servers (cross-server DMs)
- [ ] Offline LAN party mode without Internet
- [ ] Proximity voice channels for events
