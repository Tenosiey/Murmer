# 📝 TODO List

An overview of planned features for the project.
Use the checkboxes to track progress.

---

## 🚀 Features

### 🗨️ Chat Features

- [ ] Bot integration/webhooks (basic REST bot API exists, no webhooks)
- [x] Channel categories in the client UI (grouping, collapse, context-menu management)
- [x] Channel description/topics (persisted on the server, synced to all clients)
- [x] Channel permissions and moderation (role-gated kick/ban/mute with persistence)
- [ ] Custom server emojis
- [x] Direct messages between users
- [x] Edit sent messages
- [x] File sharing beyond images
- [ ] Full-text message search via SQLite FTS5 (current search is an unindexed LIKE scan)
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

- [ ] Admin dashboard (online users, storage usage, moderation)
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
- [ ] Widen message IDs from i32 to i64 throughout the server (SQLite rowids are 64-bit)

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
