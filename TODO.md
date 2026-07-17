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
- [ ] Text-to-speech
- [x] Typing indicators
- [x] Unread markers and per-channel unread badges
- [ ] User nicknames per server
- [ ] User profiles/avatars

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

- [ ] Breakout rooms
- [ ] Collaborative whiteboard during voice chats
- [ ] Custom sound effects and soundboards
- [ ] Gesture recognition through webcam
- [ ] Live polling during meetings
- [ ] Meeting notes that auto-generate from voice
- [x] Noise suppression and echo cancellation
- [ ] Optional spatial/3D audio
- [ ] Record and play back voice messages
- [ ] Real-time transcription of voice to text
- [ ] Screen-share annotations
- [ ] Temporary voice channels
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

_(no known bugs yet)_

---

## 💡 Future Ideas

- [ ] AI-powered chat summarization
- [ ] Federation between Murmer servers (cross-server DMs)
- [ ] Offline LAN party mode without Internet
- [ ] Proximity voice channels for events

---

✅ Tasks can be checked directly in the browser when completed.
