# TODO vs. Codebase Comparison

This document compares every entry in `TODO.md` with the current implementation. Columns show the status recorded in the TODO list, supporting code evidence (or lack thereof), and an assessment.

## Chat Features

| Item | TODO Status | Code Evidence | Assessment |
| --- | --- | --- | --- |
| Add reactions with emojis | ✅【F:TODO.md†L12-L12】 | Reaction payloads are normalised in the chat store and rendered with toggleable chips in the UI.【F:murmer_client/src/lib/stores/chat.ts†L30-L280】【F:murmer_client/src/routes/chat/+page.svelte†L2004-L2020】 | Matches TODO ✅ |
| Bot integration/webhooks | ⛔【F:TODO.md†L13-L13】 | Server message handling supports presence, chat, history, channel, and voice events only; there is no webhook/bot integration path.【F:murmer_server/src/ws.rs†L1-L40】 | Matches TODO ⛔ |
| Channel categories and folders | ⛔【F:TODO.md†L14-L14】 | Channel state is a flat string array without grouping support, confirming folders are absent.【F:murmer_client/src/lib/stores/channels.ts†L1-L33】 | Matches TODO ⛔ |
| Channel description/topics | ✅【F:TODO.md†L15-L15】 | Topics are stored per channel and edited via slash command/header controls.【F:murmer_client/src/lib/stores/channelTopics.ts†L1-L18】【F:murmer_client/src/routes/chat/+page.svelte†L1090-L1099】 | Matches TODO ✅ |
| Channel permissions and moderation | ⛔【F:TODO.md†L16-L16】 | The server only restricts channel management to privileged roles when `ADMIN_TOKEN` is configured; broader moderation flows remain unimplemented.【F:murmer_server/src/ws.rs†L304-L323】 | Partially implemented – further work required ⚠️ |
| Code syntax highlighting | ✅【F:TODO.md†L17-L17】 | Markdown rendering delegates to `highlight.js` for code blocks before sanitising HTML.【F:murmer_client/src/lib/markdown.ts†L1-L39】 | Matches TODO ✅ |
| Direct messages between users | ⛔【F:TODO.md†L18-L18】 | Message models only target named channels and no DM routing exists in the stores or server handlers.【F:murmer_client/src/lib/types.ts†L1-L31】【F:murmer_server/src/ws.rs†L1-L40】 | Matches TODO ⛔ |
| Enable deleting messages | ✅【F:TODO.md†L19-L19】 | Users can request deletions and the store removes messages after server confirmation.【F:murmer_client/src/routes/chat/+page.svelte†L1088-L1106】【F:murmer_client/src/lib/stores/chat.ts†L263-L344】 | Matches TODO ✅ |
| Ephemeral messages that auto-delete | ✅【F:TODO.md†L20-L20】 | Ephemeral messages clamp expiry on the server and schedule timed removal broadcasts.【F:murmer_client/src/routes/chat/+page.svelte†L847-L889】【F:murmer_server/src/ws.rs†L1005-L1073】 | Matches TODO ✅ |
| File sharing beyond images | ⛔【F:TODO.md†L21-L21】 | The upload endpoint explicitly restricts MIME types and extensions to images only.【F:murmer_server/src/upload.rs†L19-L94】 | Matches TODO ⛔ |
| Keyboard shortcuts | ✅【F:TODO.md†L22-L22】 | Global shortcuts toggle focus mode, mute states, settings, and voice joins.【F:murmer_client/src/routes/chat/+page.svelte†L1195-L1219】 | Matches TODO ✅ |
| Load chat history only for the selected channel | ✅【F:TODO.md†L23-L23】 | Switching channels clears current history and requests channel-specific backfill.【F:murmer_client/src/routes/chat/+page.svelte†L974-L982】【F:murmer_client/src/lib/stores/chat.ts†L269-L311】 | Matches TODO ✅ |
| Message history persistence | ⛔【F:TODO.md†L24-L24】 | Messages are stored in Postgres on send and replayed via `/load-history`, so persistence is already implemented.【F:murmer_server/src/ws.rs†L1005-L1073】【F:murmer_server/src/db.rs†L99-L165】 | **Mismatch – feature implemented** ✅ |
| Message threading/replies | ⛔【F:TODO.md†L25-L25】 | Message payloads have no parent/thread metadata and the UI renders a flat list only.【F:murmer_client/src/lib/types.ts†L1-L31】【F:murmer_client/src/routes/chat/+page.svelte†L1930-L2027】 | Matches TODO ⛔ |
| Message timestamps | ✅【F:TODO.md†L26-L26】 | Incoming messages normalise timestamps, ensuring time strings render for every entry.【F:murmer_client/src/lib/stores/chat.ts†L43-L80】 | Matches TODO ✅ |
| Notification settings per channel | ✅【F:TODO.md†L27-L27】 | Users can configure per-channel notification modes stored in `localStorage` and applied to mention logic.【F:murmer_client/src/lib/stores/channelNotifications.ts†L1-L93】【F:murmer_client/src/routes/chat/+page.svelte†L1688-L1723】 | Matches TODO ✅ |
| Pin important messages in a channel | ✅【F:TODO.md†L28-L28】 | Local pin storage keeps channel-scoped lists and the UI offers pin/unpin controls and a pinned tray.【F:murmer_client/src/lib/stores/pins.ts†L1-L133】【F:murmer_client/src/routes/chat/+page.svelte†L1900-L2027】 | Matches TODO ✅ |
| Search chat history | ✅【F:TODO.md†L29-L29】 | Slash command and header button open the search overlay and call the server-side search API.【F:murmer_client/src/routes/chat/+page.svelte†L905-L959】【F:murmer_client/src/lib/stores/chat.ts†L269-L311】 | Matches TODO ✅ |
| Slash commands for quick actions | ✅【F:TODO.md†L30-L30】 | `/help`, `/me`, `/shrug`, `/topic`, `/status`, `/focus`, `/ephemeral`, and `/search` are parsed client-side and dispatched appropriately.【F:murmer_client/src/routes/chat/+page.svelte†L720-L900】 | Matches TODO ✅ |
| Support Markdown formatting in text chat | ✅【F:TODO.md†L31-L31】 | Messages render via the Markdown pipeline with sanitisation and code highlighting.【F:murmer_client/src/lib/markdown.ts†L1-L39】【F:murmer_client/src/routes/chat/+page.svelte†L1951-L1976】 | Matches TODO ✅ |
| Text-to-speech | ⛔【F:TODO.md†L32-L32】 | Voice handling focuses on WebRTC streaming, VAD, and PTT with no TTS pathways or synthesis hooks.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| User nicknames per server | ⛔【F:TODO.md†L33-L33】 | A single session display name is reused across all servers with no per-server overrides.【F:murmer_client/src/lib/stores/session.ts†L1-L17】【F:murmer_client/src/routes/servers/+page.svelte†L154-L160】 | Matches TODO ⛔ |
| User profiles/avatars | ⛔【F:TODO.md†L34-L34】 | UI derives avatars from initials only; there is no profile/asset management in the stores or server.【F:murmer_client/src/routes/servers/+page.svelte†L154-L160】 | Matches TODO ⛔ |
| User status indicators (away, busy, etc.) | ✅【F:TODO.md†L35-L35】 | Status store tracks presence values, broadcasts updates, and the header menu lets users change them.【F:murmer_client/src/lib/stores/status.ts†L1-L74】【F:murmer_client/src/routes/chat/+page.svelte†L1519-L1699】 | Matches TODO ✅ |

## Voice Features

| Item | TODO Status | Code Evidence | Assessment |
| --- | --- | --- | --- |
| Add mute microphone and mute output button | ✅【F:TODO.md†L39-L39】 | Voice controls expose microphone/output mute toggles with state stored in `settings`.【F:murmer_client/src/routes/chat/+page.svelte†L1451-L1499】【F:murmer_client/src/lib/stores/settings.ts†L50-L111】 | Matches TODO ✅ |
| Add quality bars to display connection strength | ✅【F:TODO.md†L40-L40】 | Connection bars component renders five-level strength indicators for chat and voice lists.【F:murmer_client/src/lib/components/ConnectionBars.svelte†L1-L23】【F:murmer_client/src/routes/chat/+page.svelte†L1400-L1441】 | Matches TODO ✅ |
| Add volume of other users slider | ⛔【F:TODO.md†L41-L41】 | Voice roster offers per-user sliders backed by persisted `userVolumes`, so the feature is already present.【F:murmer_client/src/routes/chat/+page.svelte†L2168-L2192】【F:murmer_client/src/lib/stores/settings.ts†L77-L111】 | **Mismatch – feature implemented** ✅ |
| Breakout rooms | ⛔【F:TODO.md†L42-L42】 | Voice channel store represents a flat list with no subgrouping or breakout concepts.【F:murmer_client/src/lib/stores/voiceChannels.ts†L1-L56】 | Matches TODO ⛔ |
| Collaborative whiteboard during voice chats | ⛔【F:TODO.md†L43-L43】 | Voice modules are limited to audio signalling and contain no whiteboard integrations.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Custom sound effects and soundboards | ⛔【F:TODO.md†L44-L44】 | No code paths mix extra audio tracks or trigger effect playback within voice sessions.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Gesture recognition through webcam | ⛔【F:TODO.md†L45-L45】 | The client has no webcam access or gesture processing; voice logic is audio-only.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Live polling during meetings | ⛔【F:TODO.md†L46-L46】 | UI and stores expose no polling interfaces or vote tracking.【F:murmer_client/src/routes/chat/+page.svelte†L1390-L2195】 | Matches TODO ⛔ |
| Meeting notes that auto-generate from voice | ⛔【F:TODO.md†L47-L47】 | Voice streams are not transcribed or summarised anywhere in the codebase.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Noise suppression and echo cancellation | ⛔【F:TODO.md†L48-L48】 | Audio processing pipeline adjusts gain for VAD/PTT but lacks DSP modules for suppression or echo cancel.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Optional spatial/3D audio | ⛔【F:TODO.md†L49-L49】 | WebRTC configuration sets bitrate only; no spatial audio nodes are created.【F:murmer_client/src/lib/voice/manager.ts†L168-L198】 | Matches TODO ⛔ |
| Push-to-talk and voice activity detection | ⛔【F:TODO.md†L50-L50】 | Settings expose VAD/PTT modes and the voice manager manages key capture and VAD detectors, so functionality already exists.【F:murmer_client/src/lib/components/SettingsModal.svelte†L158-L200】【F:murmer_client/src/lib/voice/manager.ts†L61-L156】 | **Mismatch – feature implemented** ✅ |
| Record and play back voice messages | ⛔【F:TODO.md†L51-L51】 | Voice manager streams live audio only and lacks recording or playback routines.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Real-time transcription of voice to text | ⛔【F:TODO.md†L52-L52】 | No transcription APIs or text output are wired into the voice workflow.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Temporary voice channels | ⛔【F:TODO.md†L53-L53】 | Voice channels persist unless manually deleted; no TTL or auto-cleanup logic exists.【F:murmer_client/src/lib/stores/voiceChannels.ts†L63-L86】 | Matches TODO ⛔ |
| Virtual backgrounds | ⛔【F:TODO.md†L54-L54】 | The client never accesses video devices, so virtual background support is absent.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Voice activity heatmaps | ⛔【F:TODO.md†L55-L55】 | Voice UI only shows live indicators, not aggregated heatmaps.【F:murmer_client/src/lib/stores/voiceSpeaking.ts†L1-L38】 | Matches TODO ⛔ |
| Voice activity indicators | ✅【F:TODO.md†L56-L56】 | Speaking indicators toggle per user using the `remoteSpeaking` store and roster styling.【F:murmer_client/src/lib/stores/voiceSpeaking.ts†L1-L38】【F:murmer_client/src/routes/chat/+page.svelte†L1407-L1432】 | Matches TODO ✅ |
| Voice channels with different quality settings | ⛔【F:TODO.md†L57-L57】 | Voice channel creation/config menus expose quality presets and propagate bitrate to peers.【F:murmer_client/src/lib/stores/voiceChannels.ts†L63-L86】【F:murmer_client/src/lib/voice/manager.ts†L168-L198】 | **Mismatch – feature implemented** ✅ |
| Voice-controlled commands | ⛔【F:TODO.md†L58-L58】 | No speech recognition or command routing is present in the voice stack.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Voice effects and filters | ⛔【F:TODO.md†L59-L59】 | The audio pipeline forwards raw microphone input without applying filters.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Voice sentiment analysis | ⛔【F:TODO.md†L60-L60】 | There is no analytics layer over voice streams to infer sentiment.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |

## Other Features

| Item | TODO Status | Code Evidence | Assessment |
| --- | --- | --- | --- |
| Anonymous chat modes | ⛔【F:TODO.md†L64-L64】 | The client requires a stored display name for all interactions; no anonymous toggle exists.【F:murmer_client/src/lib/stores/session.ts†L1-L17】 | Matches TODO ⛔ |
| Customizable user roles & permissions | ⛔【F:TODO.md†L65-L65】 | An admin endpoint can assign roles but there is no broader permission management UI, indicating the feature is still incomplete.【F:murmer_server/src/admin.rs†L1-L65】 | Matches TODO ⛔ |
| Decentralized/mesh networking option | ⛔【F:TODO.md†L66-L66】 | All networking flows through the central Axum server; no mesh logic is present.【F:murmer_server/src/main.rs†L1-L119】 | Matches TODO ⛔ |
| Desktop notifications for @mentions | ✅【F:TODO.md†L67-L67】 | Mentions trigger desktop/Tauri notifications when channel preferences allow it.【F:murmer_client/src/lib/stores/chat.ts†L136-L158】【F:murmer_client/src/lib/notify.ts†L1-L34】 | Matches TODO ✅ |
| End-to-end encryption for private channels | ⛔【F:TODO.md†L68-L68】 | The presence handshake signs nonces but messages are sent in plaintext WebSocket frames; no E2E layer exists.【F:murmer_server/src/ws.rs†L1-L1073】 | Matches TODO ⛔ |
| Focus mode (minimal distractions) | ✅【F:TODO.md†L69-L69】 | Focus mode state is stored and toggled via header controls and shortcuts.【F:murmer_client/src/lib/stores/layout.ts†L1-L32】【F:murmer_client/src/routes/chat/+page.svelte†L1684-L1699】 | Matches TODO ✅ |
| Implement auto updates | ⛔【F:TODO.md†L70-L70】 | Settings can check GitHub releases manually, but no automatic download/install logic exists.【F:murmer_client/src/lib/components/SettingsModal.svelte†L29-L53】 | Matches TODO ⛔ |
| Mini-games embedded in chat | ⛔【F:TODO.md†L71-L71】 | The chat interface has no mini-game components or commands.【F:murmer_client/src/routes/chat/+page.svelte†L1390-L2195】 | Matches TODO ⛔ |
| Music streaming from local files | ⛔【F:TODO.md†L72-L72】 | No modules access local file libraries or stream audio into channels.【F:murmer_client/src/lib/voice/manager.ts†L51-L198】 | Matches TODO ⛔ |
| Pomodoro timer integration for study groups | ⛔【F:TODO.md†L73-L73】 | There are no timers or productivity integrations in the UI or stores.【F:murmer_client/src/routes/chat/+page.svelte†L1390-L2195】 | Matches TODO ⛔ |
| Real-time collaborative code editing | ⛔【F:TODO.md†L74-L74】 | Chat renders code snippets only; there is no collaborative editor or OT/CRDT logic.【F:murmer_client/src/lib/markdown.ts†L1-L39】 | Matches TODO ⛔ |
| Scheduled voice events / calendar integration | ⛔【F:TODO.md†L75-L75】 | Voice channels lack scheduling metadata or calendar hooks.【F:murmer_client/src/lib/stores/voiceChannels.ts†L1-L89】 | Matches TODO ⛔ |
| Server invite links | ✅【F:TODO.md†L76-L76】 | Users can generate and parse `murmer://` invites from the server list screen.【F:murmer_client/src/routes/servers/+page.svelte†L107-L140】【F:murmer_client/src/lib/invite.ts†L10-L55】 | Matches TODO ✅ |
| Theme customization (dark/light) | ✅【F:TODO.md†L77-L77】 | The theme store toggles light/dark themes and persists the selection.【F:murmer_client/src/lib/stores/theme.ts†L1-L43】【F:murmer_client/src/routes/chat/+page.svelte†L1538-L1614】 | Matches TODO ✅ |
| Sanitize uploaded filenames to prevent path traversal | ✅【F:TODO.md†L78-L78】 | Upload logic sanitises filenames and enforces type/size validation before storing files.【F:murmer_server/src/upload.rs†L63-L109】 | Matches TODO ✅ |
| Translation services for international teams | ⛔【F:TODO.md†L79-L79】 | No translation APIs or locale toggles exist in the client or server.【F:murmer_client/src/routes/chat/+page.svelte†L1390-L2195】 | Matches TODO ⛔ |

Overall, several unchecked items in `TODO.md`—notably message persistence, per-user volume controls, push-to-talk/VAD, and voice quality presets—are already implemented in the codebase.
