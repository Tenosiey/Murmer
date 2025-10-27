# Murmer Manual Test Checklist

Use this checklist to exercise every feature exposed by the current codebase. Complete it after provisioning PostgreSQL, the Axum server, and the Tauri/Svelte client.

## 1. Environment Preparation
- [ ] Launch PostgreSQL with the schema created by `murmer_server` and export `DATABASE_URL`; start the server so `/ws`, `/upload`, `/role`, and static `/files` are reachable for manual testing.【F:murmer_server/src/main.rs†L1-L119】【F:murmer_server/src/db.rs†L1-L66】
- [ ] Confirm optional environment flags such as `SERVER_PASSWORD`, `ADMIN_TOKEN`, and `CORS_ALLOW_ORIGINS` are set appropriately for the scenarios you plan to exercise.【F:murmer_server/src/main.rs†L1-L88】
- [ ] Run the Svelte/Tauri client (`npm run dev` or `npm run tauri dev`) and ensure the root route immediately redirects to `/login`.【F:murmer_client/src/routes/+page.svelte†L1-L13】

## 2. Authentication & Session Flow
- [ ] From `/login`, submit a display name and verify the app stores it in the session store and navigates to `/servers`. Refresh to confirm persistence via `localStorage`.【F:murmer_client/src/routes/login/+page.svelte†L6-L69】【F:murmer_client/src/lib/stores/session.ts†L1-L17】
- [ ] Attempt to revisit `/login` while authenticated and confirm the guard routes back to `/servers`.【F:murmer_client/src/routes/login/+page.svelte†L13-L23】
- [ ] Trigger the logout control on the servers screen and ensure both navigation and session clearing occur.【F:murmer_client/src/routes/servers/+page.svelte†L90-L97】
- [ ] Visit `/chat` without an active session to validate the redirect back to `/login`.【F:murmer_client/src/routes/chat/+page.svelte†L663-L718】

## 3. Server Directory & Settings
- [ ] Add a server using a raw host, verifying URL normalization and optional password handling before it is persisted locally.【F:murmer_client/src/routes/servers/+page.svelte†L43-L83】【F:murmer_client/src/lib/stores/servers.ts†L1-L75】【F:murmer_client/src/lib/utils.ts†L1-L16】
- [ ] Add a server via a `murmer://` invite link and confirm the parsed name/password fields populate automatically.【F:murmer_client/src/routes/servers/+page.svelte†L55-L69】【F:murmer_client/src/lib/invite.ts†L10-L55】
- [ ] Observe the reachability dot for each saved server updating after the background status checks run.【F:murmer_client/src/routes/servers/+page.svelte†L14-L26】【F:murmer_client/src/lib/stores/serverStatus.ts†L1-L60】【F:murmer_client/src/lib/components/StatusDot.svelte†L1-L15】
- [ ] Use the “Copy invite” control and ensure the generated `murmer://` link is placed on the clipboard (fallback textarea path should be covered when clipboard APIs are unavailable).【F:murmer_client/src/routes/servers/+page.svelte†L107-L140】【F:murmer_client/src/lib/invite.ts†L10-L55】
- [ ] Remove a server entry and verify it disappears from the list and from `localStorage`.【F:murmer_client/src/routes/servers/+page.svelte†L90-L92】【F:murmer_client/src/lib/stores/servers.ts†L40-L57】
- [ ] Open the Settings modal and validate audio volume, input/output device selection, and voice-mode/VAD/PTT controls all update their respective stores with persisted values.【F:murmer_client/src/lib/components/SettingsModal.svelte†L1-L200】【F:murmer_client/src/lib/stores/settings.ts†L1-L166】
- [ ] Capture a new push-to-talk key binding and ensure it propagates to the voice subsystem.【F:murmer_client/src/lib/components/SettingsModal.svelte†L67-L79】【F:murmer_client/src/lib/voice/manager.ts†L61-L156】
- [ ] Trigger the update check button and observe the GitHub release response states (pre-release, stable, none, failure).【F:murmer_client/src/lib/components/SettingsModal.svelte†L29-L53】

## 4. Chat Connection & Presence
- [ ] Choose a server and confirm the client performs the Ed25519 presence handshake (public key, timestamp, optional password) before joining channels.【F:murmer_client/src/routes/chat/+page.svelte†L663-L695】
- [ ] Verify the server accepts chat messages, persists them, and broadcasts assigned IDs (watch for inserts in the database).【F:murmer_server/src/ws.rs†L1005-L1073】【F:murmer_server/src/db.rs†L99-L165】
- [ ] Check that the ping indicator updates and drives the connection-strength bars in the header.【F:murmer_client/src/lib/stores/ping.ts†L1-L60】【F:murmer_client/src/lib/components/PingDot.svelte†L1-L16】【F:murmer_client/src/lib/components/ConnectionBars.svelte†L1-L23】
- [ ] Confirm desktop/@mention notifications appear according to per-channel preferences and mention detection.【F:murmer_client/src/lib/stores/chat.ts†L1-L158】【F:murmer_client/src/lib/stores/channelNotifications.ts†L1-L93】
- [ ] Inspect role and status broadcasts (including initial snapshots) to ensure the UI reflects server-side updates.【F:murmer_client/src/lib/stores/roles.ts†L1-L15】【F:murmer_client/src/lib/stores/status.ts†L1-L74】【F:murmer_client/src/routes/chat/+page.svelte†L1519-L1732】

## 5. Text Channel Features
- [ ] Switch channels and verify history clearing and fresh `join` events avoid duplicate backlog loading.【F:murmer_client/src/routes/chat/+page.svelte†L974-L982】
- [ ] Create and delete text channels through the context menu and ensure the list syncs via WebSocket events.【F:murmer_client/src/routes/chat/+page.svelte†L1008-L1061】【F:murmer_client/src/lib/stores/channels.ts†L1-L33】
- [ ] Edit the channel topic manually and through the `/topic` slash command, checking persistence in the header.【F:murmer_client/src/routes/chat/+page.svelte†L1090-L1099】【F:murmer_client/src/lib/stores/channelTopics.ts†L1-L18】
- [ ] Send plain, Markdown, and code-block messages and confirm rendering (including syntax highlighting).【F:murmer_client/src/lib/stores/chat.ts†L231-L344】【F:murmer_client/src/lib/markdown.ts†L1-L39】【F:murmer_client/src/routes/chat/+page.svelte†L1931-L1976】
- [ ] Upload an allowed image type, ensure the HTTP `/upload` endpoint validates it, and check that the resulting message includes the image URL.【F:murmer_client/src/routes/chat/+page.svelte†L735-L778】【F:murmer_server/src/upload.rs†L1-L116】
- [ ] Exercise slash commands (`/help`, `/me`, `/shrug`, `/topic`, `/status`, `/focus`, `/ephemeral`, `/search`) and verify their side effects and validation messages.【F:murmer_client/src/routes/chat/+page.svelte†L720-L900】
- [ ] Send an ephemeral message, wait past the expiry, and confirm the server removes it and broadcasts a deletion event.【F:murmer_client/src/routes/chat/+page.svelte†L847-L889】【F:murmer_server/src/ws.rs†L1005-L1073】
- [ ] Delete a message as its author (and as a moderator if applicable) to ensure the client sends delete requests and updates the UI.【F:murmer_client/src/routes/chat/+page.svelte†L1088-L1106】【F:murmer_client/src/lib/stores/chat.ts†L263-L344】
- [ ] Add and remove emoji reactions, including toggling your own reaction state and verifying counts update for other users.【F:murmer_client/src/lib/stores/chat.ts†L30-L280】【F:murmer_client/src/routes/chat/+page.svelte†L2004-L2020】
- [ ] Pin, unpin, and revisit pinned messages, observing summaries and jump-to-message behavior.【F:murmer_client/src/lib/stores/pins.ts†L1-L133】【F:murmer_client/src/routes/chat/+page.svelte†L1900-L2027】
- [ ] Open the search overlay (via slash command and header button), perform searches, and jump to results while confirming history loads lazily per channel.【F:murmer_client/src/routes/chat/+page.svelte†L905-L959】【F:murmer_client/src/lib/stores/chat.ts†L269-L311】
- [ ] Adjust per-channel notification preferences from the header menu and verify the stored preference drives future notifications.【F:murmer_client/src/routes/chat/+page.svelte†L1688-L1723】【F:murmer_client/src/lib/stores/channelNotifications.ts†L1-L93】
- [ ] Toggle focus mode via the header control, the global shortcut (`Ctrl/Cmd+Shift+F`), and the `/focus` command to ensure layout changes persist.【F:murmer_client/src/routes/chat/+page.svelte†L1684-L1699】【F:murmer_client/src/routes/chat/+page.svelte†L1195-L1219】【F:murmer_client/src/lib/stores/layout.ts†L1-L32】

## 6. Voice Channel Features
- [ ] Create, join, and delete voice channels; confirm quality/bitrate presets propagate and display correctly.【F:murmer_client/src/routes/chat/+page.svelte†L1008-L1045】【F:murmer_client/src/lib/stores/voiceChannels.ts†L1-L89】
- [ ] Join a voice channel and verify microphone/output mute buttons, PTT state, and VAD status lights behave as expected across modes.【F:murmer_client/src/routes/chat/+page.svelte†L1451-L1499】【F:murmer_client/src/lib/stores/settings.ts†L50-L166】【F:murmer_client/src/lib/voice/manager.ts†L51-L166】
- [ ] Observe per-user connection stats and activity indicators, including volume sliders opened from the voice roster context menu.【F:murmer_client/src/routes/chat/+page.svelte†L1400-L1442】【F:murmer_client/src/routes/chat/+page.svelte†L2160-L2195】【F:murmer_client/src/lib/stores/voiceSpeaking.ts†L1-L38】
- [ ] Test per-user volume adjustments and confirm the values persist between sessions via `userVolumes`.【F:murmer_client/src/routes/chat/+page.svelte†L2168-L2192】【F:murmer_client/src/lib/stores/settings.ts†L77-L111】
- [ ] Change voice channel quality from the context menu and verify remote peers receive updated bitrate settings.【F:murmer_client/src/routes/chat/+page.svelte†L1232-L1247】【F:murmer_client/src/lib/stores/voiceChannels.ts†L47-L86】【F:murmer_client/src/lib/voice/manager.ts†L168-L198】

## 7. Link & Media Enhancements
- [ ] Post messages containing HTTP/HTTPS links and ensure previews render (including YouTube metadata and iframe fallback after timeout).【F:murmer_client/src/lib/link-preview.ts†L1-L19】【F:murmer_client/src/lib/components/LinkPreview.svelte†L1-L118】
- [ ] Verify markdown-generated code blocks receive syntax highlighting via `highlight.js`.【F:murmer_client/src/lib/markdown.ts†L1-L39】

## 8. Roles, Status, and Admin Functions
- [ ] Use an admin token to hit `/role`, assign a role, and confirm connected clients update immediately.【F:murmer_server/src/admin.rs†L1-L65】【F:murmer_client/src/lib/stores/roles.ts†L1-L15】
- [ ] Change your status through the header menu and `/status` command; make sure the status indicators update locally and for other clients.【F:murmer_client/src/routes/chat/+page.svelte†L1519-L1699】【F:murmer_client/src/lib/stores/status.ts†L1-L74】

## 9. File Upload & Security Validations
- [ ] Attempt to upload disallowed file types or oversize payloads and confirm the server rejects them with appropriate status codes.【F:murmer_server/src/upload.rs†L19-L94】
- [ ] Review logs when exceeding message or auth rate limits to ensure throttling triggers as designed.【F:murmer_server/src/security.rs†L1-L118】
- [ ] Confirm nonce replay protection by attempting to reuse an authentication nonce and observing the rejection.【F:murmer_server/src/security.rs†L120-L169】

Completing all steps provides confidence that every documented feature operates correctly in the current build.
