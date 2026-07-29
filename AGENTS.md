# Contributor Guide

This monorepo hosts **Murmer**, a desktop chat prototype split into a
Tauri/SvelteKit client (`murmer_client/`) and an Axum-based Rust server
(`murmer_server/`). Each directory contains its own `AGENTS.md` with tooling
specifics. Client and server communicate over one WebSocket (`/ws`, JSON
frames with a `type` field) plus a few HTTP endpoints (`/upload`,
`/link-preview`, `/role`, `/files`, bot REST API).

## Hard constraints
- **TypeScript stays on major 6.** Do not upgrade to 7 or merge dependabot
  PRs that do.
- **No backwards compatibility.** Only the latest versions of everything are
  supported; never add compat shims, polyfills or legacy code paths.
- **Rust is edition 2024**; the toolchain is pinned in `rust-toolchain.toml`.
- **rusqlite is pinned by tokio-rusqlite** — bump it only when a new
  tokio-rusqlite release allows it.
- **The client toolchain is Bun 1.x.** Dependencies, scripts and CI use Bun
  (`bun install` writes `bun.lock`); there is no npm lockfile. `package.json`
  declares no `engines` (Bun ignores it).
- **Never bump versions by hand** — only via `bun run bump` (see Versioning).
- **Svelte components use the runes syntax** (`$state`, `$props`, `$derived`,
  `$effect`) — `runes: true` is enforced in `svelte.config.js`, so legacy
  syntax (`export let`, `$:`) fails the build. Shared state still lives in
  `svelte/store` modules (`src/lib/stores/`), consumed via `$store`
  auto-subscription; never import from `svelte/legacy`.

## Workflow overview
- Install the latest [Rust toolchain](https://www.rust-lang.org/tools/install)
  and [Bun 1.x](https://bun.sh).
- See `README.md` for detailed setup, build and configuration instructions.
- When developing locally run the client with `bun run tauri dev` and the server
  with `cargo run` or `docker compose up --build`.

## Quality checks
- Server: `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test` inside `murmer_server/` — all three pass clean; keep it that way.
- Client: `bun run check` inside `murmer_client/` (0 errors, 0 warnings) and
  `bun run test` (Vitest); `cargo clippy` in `murmer_client/src-tauri/` for
  the shell.
- Client unit tests sit next to their module (`src/lib/**/*.test.ts`), run in
  a plain Node environment (`vitest.config.ts`) and stub only what the stores
  need from the framework — `$app/environment` and `localStorage`, both in
  `murmer_client/test/`. Cover store logic whose failure modes are invisible
  in the UI (per-server namespacing, request/response correlation, parsing of
  server frames); do not add component-rendering tests.
- Document complex security-sensitive logic with inline comments.
- Sanitize or validate all user-supplied data before acting on it.

## Client code organisation
- `src/routes/` – SvelteKit pages (login, server selection, chat)
- `src/lib/components/` – reusable UI primitives and overlays
- `src/lib/stores/` – Svelte stores holding client state
- `src/lib/chat/` – constants and helpers shared by the chat page
- `src/lib/voice/` – WebRTC helpers, push-to-talk tooling, microphone level
  metering and the record-and-play-back mic test for the settings UI, and the
  soundboard player (local playback). The manager, the level meter and the mic
  test all open the microphone through `voice/capture.ts` so they capture the
  same signal.
  Every graph in the app shares one `AudioContext` (`voice/audioContext.ts`)
  because browsers cap how many may exist at once; the microphone chain still
  ends at its own `MediaStreamAudioDestinationNode`, so playback rendered to
  the context destination cannot leak into the outgoing track. Level meters
  are driven by an audio-worklet tick (`voice/ticker.ts`) rather than
  `requestAnimationFrame`, which stops while the window is minimised and used
  to freeze voice-activity detection with the microphone stuck open. The
  outgoing chain is `capture -> noise suppression -> input gain -> transmission
  gate -> outgoing track`: voice detection and the settings level meter tap
  *after* the input gain (the "Input volume" slider), so the level compared
  against the VAD threshold is the one the peers actually receive — turning a
  quiet microphone up must not make voice detection harder to trigger.
  Noise suppression is one of three modes (`stores/settings.ts`'s
  `noiseSuppressionMode`), never two at once: `browser` asks the platform for
  its `noiseSuppression` capture constraint, `rnnoise` (the default) runs the
  RNNoise worklet from `voice/denoise.ts` instead, and stacking them would hand
  the better filter a signal the other one already mangled. RNNoise sits ahead
  of the input gain so it sees speech at capture level, which also collapses the
  noise floor the VAD tracks. Three things it needs, all of which fail quietly
  if forgotten: the WASM binary is fetched on the main thread and passed in as
  an `ArrayBuffer` (an `AudioWorkletGlobalScope` has no `fetch`), the CSP in
  `tauri.conf.json` must keep `'wasm-unsafe-eval'` in `script-src` (the dev
  server sends no CSP, so a mistake there only surfaces in a packaged build),
  and the shared `AudioContext` must run at 48 kHz — the only rate RNNoise is
  trained for, which is why `voice/audioContext.ts` asks for it. Every failure
  path drops the node and keeps the microphone working.
  All three chains — the manager's, the settings level meter's and the
  microphone test's — build their microphone end through
  `denoise.ts::connectMicSource`, so a meter can never show a level that was
  measured on a different signal than the one being gated.
  The VAD threshold is derived from a tracked noise floor by default
  (`NoiseFloorTracker` in `voice/vad.ts`); `vadSensitivity` is only the manual
  override used when `vadAutoSensitivity` is off. The tracker drops fast and
  rises very slowly, and can never rise above the quietest level of the last
  20 s — speech returns to the room between syllables, so that cap is what
  stops a long sentence from ratcheting the threshold up under the speaker.
  Tune it towards never gating a talking user: transmitting a few more seconds
  of fan noise is the cheaper mistake. Each measurement chain runs its own
  tracker (the detector's and the settings meter's) on the same signal.
  A peer connection that breaks mid-call is **repaired, never dropped** — see
  `src/lib/webrtc/` below, which both this manager and screen sharing use.
  Two voice-side rules hold it together: `handleAnswer` gates on
  `signalingState === 'have-local-offer'` rather than on there being no remote
  description yet (a repair is a *second* round of offer/answer, so the old
  check silently dropped every one of them), and `updateStats` reports zero
  bars for anything not `connected` — an rtt of 0 otherwise reads as
  "excellent", which showed five full bars for a peer carrying no audio at all.
  Which of the two ends re-offers is decided by comparing the account names, so
  both machines pick the same side without a round trip to agree.
  Opus is negotiated with `usedtx=1;useinbandfec=1`, written into every
  offer/answer by `voice/sdp.ts`. Those fmtp parameters are *receiver-to-sender*
  preferences (RFC 7587), so the description we **send** configures the peer's
  encoder and the one we **receive** configures ours — which is why both the
  local and the remote description are munged. The rewrite is a pure,
  idempotent function that only appends to Opus fmtp lines and hands back
  anything it cannot parse; a call must still connect when munging fails.
  DTX pairs with the transmission gate, which feeds the encoder digital zero
  while muted or not transmitting: a silent uplink drops from ~50 packets/s to
  a handful (measured 50 -> 6.7). That is also why `updateStats` only
  recomputes packet loss once `MIN_LOSS_SAMPLE_PACKETS` have gone by instead of
  once per poll — dividing by the two or three packets a DTX'd stream carries
  per second turned a single loss into "50 % loss" and emptied the connection
  bars of everyone who was not talking.
- `src/lib/webrtc/` – the parts both WebRTC managers share. `recovery.ts` is
  the repair policy for a connection that breaks mid-session: `disconnected` is
  a Wi-Fi roam or a lid closed for a second and usually heals itself, so it is
  given a grace period; only if it does not resolve — or on `failed`, which
  never resolves — is ICE restarted, retried until a deadline, and finally the
  connection thrown away and rebuilt. Closing on `disconnected` (what this
  replaced) turned every brief hiccup into a peer that was still listed and
  carried no audio, or a screen-share window that shut for good. The controller
  is free of `window`, WebRTC and store imports precisely so these timings —
  the part that fails invisibly — are unit-tested against fake timers.
  `fingerprint.ts` tells the two kinds of re-offer apart: an ICE restart keeps
  the DTLS certificate and is answered on the existing connection, while a peer
  that rebuilt presents a new one and can only be answered on a new connection.
  Both managers must clear their recovery entries on every teardown path, or a
  timer fires against a session that is already gone.
- `src/lib/screenshare/` – WebRTC screen sharing manager. A share may carry
  system audio, so the viewer offers **recvonly video *and* audio**
  transceivers: an answer can only fill m-lines the offer already contains, and
  without the audio one the sharer would have nowhere to put its audio track.
  Silent shares answer it `inactive`, which is how the viewer knows whether to
  show its volume/mute controls (`currentDirection`, not `getReceivers()` —
  every transceiver owns a receiver track, including the ones carrying nothing).
  Everyone in a voice channel may share at the same time and watch each other,
  so connections are keyed by *direction as well as* peer name (`incoming` per
  sharer, `outgoing` per viewer) and every signaling frame carries the sender's
  `role`; one map keyed by name alone would hand a mutual pair of sharers a
  single connection for two sessions. Stopping your own share therefore only
  closes `outgoing` — the shares you are watching keep running.
  Offers, answers and candidates must keep carrying `target`: the server routes
  them to that peer alone instead of broadcasting, so a frame without one is
  dropped and its session never connects. The client-side `target` checks stay
  as they are — they are what makes the two ends agree on who a frame was for.
  The viewer side is a **layer of floating windows**
  (`ScreenShareLayer`/`ScreenShareWindow` over `watchedScreenShares` in
  `stores/screenShare.ts`): watching never blocks the app, so the layer is
  `pointer-events: none` and only the windows themselves take input. Each
  window is dragged, resized from any corner, shrunk into a corner
  (picture-in-picture), maximized or made fullscreen on its own, and carries
  its own volume/mute overriding the app-wide
  `screenShareVolume`/`screenShareMuted` default; the sharer's self-preview is
  one of these windows and starts in picture-in-picture. Geometry lives in
  `stores/screenShareWindows.ts`, which owns every clamping rule (a window may
  never leave the viewport — a header dragged off screen can never be grabbed
  again) and is handed the viewport by the layer instead of reading `window`,
  so all of it is testable. Layouts persist per sharer, namespaced by server
  URL like the other per-name state. A watched entry with no peer yet is still
  negotiating and stays up; one whose peer disappears has ended and is closed,
  which is what keeps a window from hanging on "Connecting…" forever.
  That reconciliation is why a share **under repair keeps being reported** by
  `getPeersList` even while it has no connection at all: dropping out of the
  peer list is the store's signal that a share ended, so a repair that stopped
  reporting would close the window it was trying to save. The retained entry in
  `remoteStreams` is what says a share is still ours — every real teardown
  deletes it, `discardIncoming` (rebuild only) deliberately does not, and the
  window keeps its last frame under a "Reconnecting…" overlay meanwhile.
  Unlike voice, which repairs indefinitely, a watched share gives up after
  `MAX_REBUILDS` and closes: a row in a member list can wait forever, a window
  on the user's screen cannot. Offers only ever travel viewer → sharer, so the
  viewer is always the side that re-offers; the sharer drops its outgoing
  connection and waits. Connections are repaired under a `role:peer` key for
  the same reason they are stored per direction — a mutual pair of sharers has
  two connections with the same person.
- `src-tauri/` – Rust-side glue for native integrations

## Security expectations
- Authentication relies on Ed25519 signatures with replay protection.
- **The key is the account, so it is backed up, not just stored.** The Ed25519
  key in `keypair.ts` authenticates on every server *and* derives the X25519
  keys DMs are encrypted to, and the server binds a name to the first key that
  claims it — losing the key loses every account at once plus the ability to
  read a single DM ever received. `identity.ts` exports it two ways from the
  same 32-byte seed: a passphrase-encrypted recovery file (PBKDF2-SHA256 over
  Web Crypto, then NaCl secretbox) that also carries the account name, and a
  24-word BIP39 phrase whose checksum catches a mistyped word before it
  restores a *valid but different* identity. An Ed25519 secret key is
  `seed || publicKey`, which is why `seedOf` can export identities created
  before any of this existed — the storage format never changed. Restoring
  replaces the key every store and connection was built around, so it confirms
  and then reloads. Nothing here may reach for `localStorage` or a store: the
  formats are a promise to anyone holding an old backup, and `identity.test.ts`
  rebuilds the file from its *documented* description rather than from the
  exporter so a drift in either one fails.
- **The account name is the identity; the display name is decoration.** A
  user's profile (`user_keys.display_name`/`about`, edited with `set-profile`,
  read from `profile-snapshot`/`profile-update`) only changes what the UI
  renders — `stores/profiles.ts` exposes `$displayNames(user)` for that. Never
  address a user by display name: auth, roles, moderation, DM routing and
  message authorship all stay on the account name, which is bound to the user's
  key on first connect and shown next to the display name on the profile.
  Display names are deliberately not unique, so any lookup by one is a bug.
- Direct messages are end-to-end encrypted (NaCl box over X25519 keys derived
  from the users' Ed25519 identity keys via ed2curve). The server only
  validates, stores and relays `nonce`/`ciphertext` pairs — it must never
  gain a plaintext DM path. Clients pin peer keys on first use
  (`stores/peerKeys.ts`), block sending on key changes until the user trusts
  the new key, and expose a fingerprint for out-of-band verification.
- Rate limiting exists for both authentication and chat traffic.
- File uploads are validated by size and an extension safe-list; images are
  additionally checked by magic bytes. Active content (HTML, SVG, scripts) is
  never accepted. The safe-list is grouped into categories (images, documents,
  archives, audio, video) in `murmer_server/src/upload.rs` and mirrored in
  `murmer_client/src/lib/chat/constants.ts`. Which categories are accepted and
  the per-file size cap are server settings (`MANAGE_SERVER`, Server Dashboard →
  Files & Uploads) persisted in `server_settings` and read by `/upload` on every
  request. Settings can only *narrow* the safe-list: unknown category ids are
  rejected on write and dropped on read, so no setting can admit active content.
  The client copy is cosmetic (picker `accept`, pre-upload check) but is held
  to the server's list by `murmer_client/test/server-mirror.test.ts`, which
  also asserts neither copy ever admits active content.
- Authorization is a **permission bitmask**, not fixed roles. Server owners
  define custom roles in the Server Dashboard and toggle each capability
  (view/send/manage channels/kick/ban/manage roles/…) per role. A user's
  effective permissions are the union of the built-in `@everyone` baseline
  role and every role assigned to them; `ADMINISTRATOR` (the Owner role) grants
  everything. The flag set is defined in `murmer_server/src/permissions.rs` and
  mirrored in `murmer_client/src/lib/chat/permissions.ts` — keep them in sync;
  `murmer_client/test/server-mirror.test.ts` parses the Rust source and fails
  when the two drift.
  A role may also carry an **icon**: an upload URL (`/files/<key>`) pointing at
  an uploaded image or an existing custom emoji's file, validated on write like
  the server icon (image safe-list, file must exist, size cap) and sanitized
  again client-side before it is used as an image source. Replaced icons are
  never deleted from disk because the same file may back a custom emoji or
  another role.
  Roles stack (a user may hold several) and carry a hierarchy `position`;
  moderation and role management require strictly outranking the target, and a
  manager can never grant a permission it lacks. Every check is enforced
  server-side (`ws/helpers.rs::has_permission`/`top_position`); client gating
  is cosmetic. Without `ADMIN_TOKEN`, channel and wiki management stay open to
  everyone so a small unadministered server remains usable.
- **Private channels** layer per-channel allow/deny overrides (for `@everyone`,
  roles and individual users) on top of the server-wide permissions, resolved
  by `channel_permissions`/`can_view_channel` in `ws/helpers.rs`. Overrides only
  touch "see" (`VIEW_CHANNELS`) and "write/talk" (`SEND_MESSAGES`). The server
  hides invisible channels from listings, filters channel-scoped broadcasts per
  recipient (the `global_rx` loop in `ws/handlers/mod.rs`), and refuses
  join/history/send/voice-join for channels a user cannot see. Voice **talk** is
  the one client-enforced piece (mic disabled via the `voice-permissions` hint)
  because audio is peer-to-peer; view/join and all text gates are server-enforced.
  Managers (`MANAGE_CHANNELS`) edit overrides via the `set/remove-channel-override`
  frames (`ws/handlers/channel_overrides.rs`); override data is sent only to
  managers.
- **Soundboard** sounds are a server-wide shared library gated by two
  permissions: `MANAGE_SOUNDS` (upload/rename/delete) and `USE_SOUNDBOARD`
  (play), the latter part of the `@everyone` baseline. Playback is *local on
  every listener*: the server only authorizes `play-sound` and fans out a
  `soundboard-play` frame; each client fetches and plays the file itself and
  nothing is ever mixed into a microphone stream. That is what makes the
  per-listener volume, per-sound mute and per-user mute (all local, persisted
  per server URL) possible. Uploads reuse `/upload` and are re-validated on
  registration by extension, size (`MAX_SOUND_FILE_BYTES`) and audio magic
  bytes (`upload.rs::detect_audio_type`) because these clips auto-play on
  everyone's machine. Playback carries a **server-side per-user cooldown**
  (`SOUNDBOARD_COOLDOWN_MS`); the client's is a cosmetic mirror. Server-muted
  members cannot play sounds either.
- Lifetime user stats are double opt-in: recording requires the server-wide
  toggle (Owner/Admin) AND the user's own opt-in, enforced in
  `murmer_server/src/db/stats.rs`. Only aggregate counters are stored — never
  message contents or recipients. The soundboard's `play_count`/`last_played_at`
  live on the sound row instead and are deliberately unattributed, so they need
  no consent gate — never add a "who played it" column.
- Production deployments should keep CORS disabled unless explicitly required.

## Versioning
Releases use the date-based scheme `YYYY.MDD.N` (year, month+day, counter for
multiple releases on the same day), e.g. `2026.710.0` for the first release on
2026-07-10. The scheme stays semver-ordered, which the Tauri updater requires.
**Client and server share one version** and are bumped in lockstep — the
server crate (`murmer_server/Cargo.toml`) must not be bumped by hand or
skipped. When asked to bump versions:

1. Run `bun run bump` inside `murmer_client/`. The script
   (`scripts/bump-version.mjs`) computes the next version and writes it into
   all six versioned files: the client's `package.json`,
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`
   and `src-tauri/Cargo.lock`, plus the server's `Cargo.toml` and
   `Cargo.lock` (the lock files matter: `--locked` builds fail when they
   disagree). `bun.lock` needs no bump — it does not record the root version.
2. Commit with `Release v<version>` and create a matching `v<version>` git
   tag. Pushing the tag triggers the GitHub Actions release workflow, which
   builds the installers and updater manifest.

See `README.md` for the full release process.

## Validation checklist
- Ensure the commands above pass before pushing; `.github/workflows/ci.yml`
  runs the same ones on every push to `main`/`dev` and on every pull request.
- Perform manual smoke tests after changing networking, authentication or file
  handling logic.
- Keep documentation (`README.md`, `AGENTS.md`) in sync with code behaviour.
