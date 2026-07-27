# 📝 TODO List

An overview of planned features for the project.
Use the checkboxes to track progress.

---

## 🚀 Features

### 🗨️ Chat Features

- [x] Ban user via the user context menu (kick exists there already)
- [ ] Bot integration/webhooks (basic REST bot API exists, no webhooks)
- [x] Channel categories in the client UI (grouping, collapse, context-menu management)
- [x] Channel description/topics (persisted on the server, synced to all clients)
- [x] Channel permissions and moderation (role-gated kick/ban/mute with persistence)
- [x] Custom server emojis (managed in the server dashboard, usable in messages and reactions)
- [x] Custom sort order for channels and categories (drag & drop reordering, persisted server-side)
- [x] Direct messages between users
- [x] Drag & drop channels into categories (text and voice, with a drop zone for removing a channel from its category)
- [x] Edit sent messages
- [x] File sharing beyond images
- [x] Full-text message search via SQLite FTS5 (indexed over message text, kept in sync by triggers)
- [x] Link previews (OpenGraph embeds)
- [x] Message threading/replies
- [x] Paste images from clipboard / drag & drop
- [x] Quote/reply to single messages (lightweight alternative to full threading)
- [x] Pin important messages in a channel (server-persisted, synced to all clients)
- [ ] Role icons — assign an icon (custom server emoji or uploaded image) per
      role in the Server Dashboard, shown next to the user name in the member
      list and messages (roles already carry a color)
- [ ] Text-to-speech
- [x] Typing indicators
- [x] Unread markers and per-channel unread badges
- [ ] User nicknames per server
- [ ] User profiles/avatars (avatars done: uploaded per server, shown in messages, member list and DMs; profile pages pending)

### 📚 Channel Wiki

Every channel can host a Markdown-based wiki with multiple pages. Pages are
identified by a slug that is unique per channel and addressed as
`channel/page-slug`. Wiki links use `[[page]]` for pages in the same channel
and `[[channel/page]]` for pages in other channels; links to missing pages
render as "create page" stubs. Content is stored server-side in SQLite and
synced to clients; editing is role-gated like other channel management.

- [x] Server: `wiki_pages` table (channel id, slug, title, Markdown body, author, updated_at, revision counter) with per-channel slug uniqueness
- [x] Server: `wiki_revisions` table storing previous versions for history/rollback
- [x] Server: CRUD API (list pages of a channel, get, create, update, delete, rename) over the existing WS protocol, with validation and size limits on page content
- [x] Server: role-gated write permissions (reuse channel moderation roles); read access for everyone who can see the channel
- [x] Server: resolve endpoint for `[[channel/page]]` links (existence check for stub rendering) and cleanup when a channel is deleted
- [x] Server: index wiki pages in the FTS5 full-text search alongside messages (indexed with triggers; search UI hookup pending below)
- [x] Client: wiki panel/tab in the chat view with a per-channel page list and page viewer
- [x] Client: Markdown editor with live preview (reuse `src/lib/markdown.ts`), save/cancel and conflict warning on concurrent edits
- [x] Client: `[[...]]` wiki-link syntax in the Markdown renderer — same-channel and cross-channel navigation, red/stub styling for missing pages
- [x] Client: create/rename/delete pages via context menu, honoring server-side permissions
- [ ] Client: revision history view with diff and restore
- [ ] Client: wiki pages included in the search UI results
- [x] Sanitize rendered wiki HTML (same hardening as chat Markdown, no active content)

### 🎤 Voice Features

- [ ] Automatic input sensitivity — track the noise floor and derive the VAD
      threshold from it instead of asking the user to dial in a number, with a
      manual override for the cases it gets wrong
- [ ] Breakout rooms
- [ ] Collaborative whiteboard during voice chats
- [x] Custom sound effects and soundboards (shared sound library, permission-gated
      upload and playback, per-listener volume/mute and playback stats)
- [ ] Ducking — drop the soundboard (and other app sounds) while somebody is
      actually talking, so a clip never buries the conversation
- [ ] Gesture recognition through webcam
- [ ] Input volume / mic gain slider — the transmission gate in
      `voice/manager.ts` is already a gain node, so this is a multiply on the
      value it ramps to. Today automatic gain control is the only way to lift a
      quiet microphone, and it is a blunt instrument that also lifts the noise
- [ ] Live polling during meetings
- [ ] Meeting notes that auto-generate from voice
- [ ] Mic test / loopback — record a few seconds and play it back so users can
      hear what echo cancellation, noise suppression and AGC actually do to
      their voice, rather than guessing from a level bar
- [x] Noise suppression and echo cancellation
- [x] Live input level meter next to the VAD sensitivity slider so the threshold
      can be adjusted while watching one's own voice level in real time
- [x] Merge the separate Audio / Microphone / Voice settings sections into two
      tabs split by direction: "Audio" (playback, output device, soundboard)
      and "Microphone & Voice" (input device, processing, transmission mode)
- [x] Per-user volume boost up to 200% so quiet members can be turned up
      (gain node per remote stream, since an `<audio>` element caps at 100%)
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
- [ ] Share system audio with a screen share — `screenshare/manager.ts`
      hardcodes `audio: false` in its `getDisplayMedia` call
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

- [ ] Admin dashboard (dashboard shell with emoji/moderation/stats tabs exists; online users, storage usage and voice sections are still placeholders)
- [ ] Anonymous chat modes
- [ ] Backup & export of chat history and uploads
- [ ] Decentralized/mesh networking option
- [ ] End-to-end encryption for private channels
- [x] Implement auto updates
- [x] SQLite mode as alternative to PostgreSQL (single-binary deploy; SQLite is now the only backend)
- [ ] Web client (browser build without Tauri, join via invite link)
- [ ] Mini-games embedded in chat
- [ ] Music streaming from local files
- [ ] Pomodoro timer integration for study groups
- [ ] Real-time collaborative code editing
- [ ] Scheduled voice events / calendar integration
- [ ] Translation services for international teams
- [x] Widen message IDs from i32 to i64 throughout the server (SQLite rowids are 64-bit)

---

## 🔧 Tech debt / hardening

- [ ] Gate the `/upload` endpoint behind authentication (or at least IP rate
      limiting like auth) — currently anyone who can reach the server can
      write 10 MB files to disk; needs a small client change to send
      credentials with the upload
- [ ] Make the rate limiter's clock injectable so the map-sweep behaviour in
      `security.rs` can be covered by a regression test in
      `tests/security_limits.rs` (the 60 s window uses `std::time::Instant`
      directly and cannot be fast-forwarded)
- [ ] Add a JS test runner (vitest) to the client — store logic like the
      per-server unread namespacing and the wiki request tracking is complex
      enough to deserve tests

---

## 🐛 Bugs

- [ ] Crackling/popping artefacts in transmitted voice audio while speaking
- [x] Own talking indicator in the voice channel does not light up in "Always On"
      mode (works for the other transmission modes)
- [ ] Soundboard: uploading a new sound fails
- [ ] Screen share: after the streamer stops sharing, the overlay controls stop
      responding (state is not reset)
- [x] Screen share: the video is slightly cut off at the bottom edge in
      windowed (non-fullscreen) mode

---

## 💡 Future Ideas

- [ ] AI-powered chat summarization
- [ ] Federation between Murmer servers (cross-server DMs)
- [ ] Offline LAN party mode without Internet
- [ ] Proximity voice channels for events

---

✅ Tasks can be checked directly in the browser when completed.
